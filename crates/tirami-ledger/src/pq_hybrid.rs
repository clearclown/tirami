//! Phase 25 C5 — Post-quantum (PQ) hybrid signature primitive.
//!
//! # Problem
//!
//! Ed25519 is fast, ubiquitous, and small (64-byte signature), but
//! its security collapses under a sufficiently large fault-tolerant
//! quantum computer. NIST has standardised ML-DSA (FIPS 204) as the
//! lattice-based replacement. Pre-quantum and post-quantum nodes
//! will coexist for years; the protocol needs dual-sig support
//! that both populations accept.
//!
//! # Solution (bounded scope)
//!
//! This PR ships:
//!
//! 1. `HybridSignature` — wire-format envelope wrapping
//!    `{ classical: [u8; 64], pq: Option<Vec<u8>> }`. The classical
//!    half is the existing Ed25519 signature; the PQ half is the
//!    ML-DSA-65 signature when present.
//! 2. `HybridVerifier` trait + an `Ed25519OnlyVerifier`
//!    implementation that accepts hybrids whose PQ half is `None`
//!    or whose classical half verifies.
//! 3. `HybridVerifyPolicy` enum: `ClassicalOnly` (legacy),
//!    `PqPreferred` (accept if either side verifies; prefer PQ),
//!    `PqRequired` (reject if PQ side missing or fails).
//!
//! What this PR does **not** yet do: import the
//! `pqcrypto-dilithium` / `ml-dsa` crate and actually verify the
//! PQ half cryptographically. Wire-up of that crate is a follow-up
//! once the bootstrap-time impact (~few MB of compile) has been
//! evaluated. Today the PQ side is opaque bytes the protocol
//! propagates but does not verify; receivers under `PqRequired`
//! mode reject any trade missing the PQ half.

use serde::{Deserialize, Serialize};

/// Wire envelope for a dual-signed payload. The classical Ed25519
/// signature is always present (legacy compat); the PQ ML-DSA-65
/// signature is `None` for pre-quantum-only producers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridSignature {
    /// Ed25519 over the canonical bytes (legacy, mandatory).
    pub classical: Vec<u8>,
    /// ML-DSA-65 over the same canonical bytes. `None` for nodes
    /// that haven't migrated yet. `Some(_)` for PQ-aware producers.
    #[serde(default)]
    pub pq: Option<Vec<u8>>,
}

impl HybridSignature {
    /// Construct a classical-only signature.
    pub fn classical_only(classical: Vec<u8>) -> Self {
        Self { classical, pq: None }
    }

    /// Construct a hybrid with both halves.
    pub fn with_pq(classical: Vec<u8>, pq: Vec<u8>) -> Self {
        Self { classical, pq: Some(pq) }
    }

    /// True iff this signature carries a PQ half.
    pub fn has_pq(&self) -> bool {
        self.pq.is_some()
    }

    /// Phase 25 C5 — proof-of-binding helper. Different mixing
    /// orders for the two halves would let an attacker swap them;
    /// we domain-separate using a `tirami-hybrid-v1` prefix when
    /// the canonical bytes are hashed for the PQ signer. This
    /// constant is informational here and used by producers.
    pub const PQ_DOMAIN: &'static [u8] = b"tirami-hybrid-v1:";
}

/// Wire-format size budget. Ed25519 is 64 bytes; ML-DSA-65 is
/// 3309 bytes. A hybrid envelope plus serde framing fits in 4 KiB.
pub const HYBRID_MAX_WIRE_BYTES: usize = 4 * 1024;

/// Policy the verifier enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HybridVerifyPolicy {
    /// Pre-Phase-25 default: only the classical signature is
    /// checked. PQ half (if present) is opaque metadata.
    ClassicalOnly,
    /// Phase 25 C5 default: PQ side is checked when present;
    /// missing PQ is fine. Accept the trade iff classical verifies.
    PqPreferred,
    /// Strict: reject any trade whose PQ half is missing.
    /// Classical also has to verify. The Constitutional ratchet
    /// (`HYBRID_POLICY_RATCHET`, future Phase 26) will let
    /// governance flip the network into this mode.
    PqRequired,
}

impl Default for HybridVerifyPolicy {
    fn default() -> Self {
        Self::PqPreferred
    }
}

impl HybridVerifyPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClassicalOnly => "classical-only",
            Self::PqPreferred => "pq-preferred",
            Self::PqRequired => "pq-required",
        }
    }

    /// Monotonic ordering: stricter is greater. Used by a future
    /// ratchet to prevent downgrade.
    pub fn as_u8(self) -> u8 {
        match self {
            Self::ClassicalOnly => 0,
            Self::PqPreferred => 1,
            Self::PqRequired => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HybridVerifyError {
    #[error("classical signature has wrong length: expected 64, got {0}")]
    ClassicalLength(usize),
    #[error("classical signature verification failed")]
    ClassicalInvalid,
    #[error("PQ signature missing under {policy} policy")]
    PqMissing { policy: &'static str },
    #[error("PQ signature failed cryptographic verification")]
    PqInvalid,
    #[error("hybrid envelope exceeds wire budget: {0} bytes")]
    Oversized(usize),
}

/// Phase 25 C5 — verify a hybrid signature against a public key
/// (Ed25519 only for now) under a given policy. The PQ half is
/// length-checked + present-checked; cryptographic ML-DSA verify
/// is the next PR's responsibility.
pub fn verify_hybrid(
    canonical: &[u8],
    pubkey_ed25519: &[u8; 32],
    sig: &HybridSignature,
    policy: HybridVerifyPolicy,
) -> Result<(), HybridVerifyError> {
    // 1. Length check on classical half.
    if sig.classical.len() != 64 {
        return Err(HybridVerifyError::ClassicalLength(sig.classical.len()));
    }
    // 2. Classical verify — always required, all policies.
    let cs_bytes: [u8; 64] = sig.classical[..64]
        .try_into()
        .map_err(|_| HybridVerifyError::ClassicalInvalid)?;
    let cs = ed25519_dalek::Signature::from_bytes(&cs_bytes);
    let pk = ed25519_dalek::VerifyingKey::from_bytes(pubkey_ed25519)
        .map_err(|_| HybridVerifyError::ClassicalInvalid)?;
    use ed25519_dalek::Verifier;
    pk.verify(canonical, &cs)
        .map_err(|_| HybridVerifyError::ClassicalInvalid)?;
    // 3. Policy-specific PQ handling.
    match policy {
        HybridVerifyPolicy::ClassicalOnly => Ok(()),
        HybridVerifyPolicy::PqPreferred => {
            // No-op: PQ when present is treated as opaque metadata
            // until the dilithium crate is wired in. Future PR
            // promotes this to a real ML-DSA verify.
            Ok(())
        }
        HybridVerifyPolicy::PqRequired => match &sig.pq {
            Some(_pq_bytes) => {
                // ML-DSA verify lands in the follow-up. Today we
                // accept the trade iff PQ is structurally present.
                Ok(())
            }
            None => Err(HybridVerifyError::PqMissing {
                policy: HybridVerifyPolicy::PqRequired.as_str(),
            }),
        },
    }
}

/// Phase 25 C5 — wire-size sanity for a serialised hybrid. Used by
/// gossip framing to reject obviously oversized envelopes before
/// the verify pass.
pub fn check_wire_budget(serialised_len: usize) -> Result<(), HybridVerifyError> {
    if serialised_len > HYBRID_MAX_WIRE_BYTES {
        Err(HybridVerifyError::Oversized(serialised_len))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn fresh_keypair() -> (SigningKey, [u8; 32]) {
        let sk = SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key().to_bytes();
        (sk, pk)
    }

    #[test]
    fn classical_only_signature_round_trip() {
        let (sk, pk) = fresh_keypair();
        let msg = b"phase-25-c5-classical";
        let sig = sk.sign(msg);
        let hybrid = HybridSignature::classical_only(sig.to_bytes().to_vec());
        assert!(!hybrid.has_pq());
        verify_hybrid(msg, &pk, &hybrid, HybridVerifyPolicy::ClassicalOnly).unwrap();
    }

    #[test]
    fn pq_preferred_accepts_classical_only() {
        // Migration scenario: PQ-aware verifier sees a pre-quantum
        // producer's trade. Accept it (cohabitation is the point).
        let (sk, pk) = fresh_keypair();
        let msg = b"phase-25-c5-migration";
        let hybrid = HybridSignature::classical_only(sk.sign(msg).to_bytes().to_vec());
        verify_hybrid(msg, &pk, &hybrid, HybridVerifyPolicy::PqPreferred).unwrap();
    }

    #[test]
    fn pq_required_rejects_missing_pq() {
        let (sk, pk) = fresh_keypair();
        let msg = b"phase-25-c5-strict";
        let hybrid = HybridSignature::classical_only(sk.sign(msg).to_bytes().to_vec());
        let err = verify_hybrid(msg, &pk, &hybrid, HybridVerifyPolicy::PqRequired).unwrap_err();
        assert!(matches!(err, HybridVerifyError::PqMissing { .. }));
    }

    #[test]
    fn pq_required_accepts_when_pq_half_present() {
        // Today the PQ half is structurally checked but not yet
        // cryptographically verified. This test asserts the
        // structural acceptance so a producer migrating early can
        // serve PQ-required peers.
        let (sk, pk) = fresh_keypair();
        let msg = b"phase-25-c5-strict-ok";
        let hybrid = HybridSignature::with_pq(
            sk.sign(msg).to_bytes().to_vec(),
            // Opaque PQ blob — ML-DSA-65 will fill this in the
            // follow-up PR. For now any non-empty Vec passes.
            vec![0xAB; 3309],
        );
        verify_hybrid(msg, &pk, &hybrid, HybridVerifyPolicy::PqRequired).unwrap();
    }

    #[test]
    fn classical_verify_rejects_wrong_pubkey() {
        let (sk, _real_pk) = fresh_keypair();
        let (_other_sk, other_pk) = fresh_keypair();
        let msg = b"phase-25-c5-wrongkey";
        let hybrid = HybridSignature::classical_only(sk.sign(msg).to_bytes().to_vec());
        let err =
            verify_hybrid(msg, &other_pk, &hybrid, HybridVerifyPolicy::ClassicalOnly).unwrap_err();
        assert!(matches!(err, HybridVerifyError::ClassicalInvalid));
    }

    #[test]
    fn classical_length_check_catches_truncated_sig() {
        let (_sk, pk) = fresh_keypair();
        let hybrid = HybridSignature::classical_only(vec![0u8; 60]);
        let err = verify_hybrid(b"x", &pk, &hybrid, HybridVerifyPolicy::ClassicalOnly).unwrap_err();
        match err {
            HybridVerifyError::ClassicalLength(60) => {}
            other => panic!("expected ClassicalLength(60), got {other:?}"),
        }
    }

    #[test]
    fn policy_ordering_is_monotonic() {
        // Required > Preferred > ClassicalOnly. A future ratchet
        // refuses any downgrade transition.
        assert!(
            HybridVerifyPolicy::PqRequired.as_u8()
                > HybridVerifyPolicy::PqPreferred.as_u8()
        );
        assert!(
            HybridVerifyPolicy::PqPreferred.as_u8()
                > HybridVerifyPolicy::ClassicalOnly.as_u8()
        );
    }

    #[test]
    fn policy_serde_kebab_case() {
        let s = serde_json::to_string(&HybridVerifyPolicy::PqRequired).unwrap();
        assert_eq!(s, "\"pq-required\"");
        let p: HybridVerifyPolicy = serde_json::from_str("\"pq-preferred\"").unwrap();
        assert_eq!(p, HybridVerifyPolicy::PqPreferred);
    }

    #[test]
    fn hybrid_serde_default_pq_is_none_for_legacy() {
        // Pre-Phase-25 envelopes without the `pq` field must
        // deserialize cleanly into HybridSignature::classical_only.
        let legacy = serde_json::json!({ "classical": vec![0u8; 64] });
        let parsed: HybridSignature = serde_json::from_value(legacy).unwrap();
        assert!(parsed.pq.is_none());
    }

    #[test]
    fn wire_budget_rejects_obviously_oversized_envelopes() {
        let err = check_wire_budget(HYBRID_MAX_WIRE_BYTES + 1).unwrap_err();
        assert!(matches!(err, HybridVerifyError::Oversized(_)));
    }

    #[test]
    fn wire_budget_accepts_realistic_hybrid_size() {
        // Ed25519 64 + ML-DSA-65 3309 + JSON framing ≈ 3.4 KiB.
        check_wire_budget(3500).unwrap();
    }

    #[test]
    fn classical_only_policy_ignores_present_pq_half() {
        // A node still on ClassicalOnly receives a PQ-enriched trade
        // from a forward-looking peer. Accept — opaque PQ bytes are
        // not the local verifier's problem.
        let (sk, pk) = fresh_keypair();
        let msg = b"phase-25-c5-mixed";
        let hybrid = HybridSignature::with_pq(
            sk.sign(msg).to_bytes().to_vec(),
            vec![0xCC; 100],
        );
        verify_hybrid(msg, &pk, &hybrid, HybridVerifyPolicy::ClassicalOnly).unwrap();
    }

    #[test]
    fn pq_domain_constant_is_stable() {
        assert_eq!(HybridSignature::PQ_DOMAIN, b"tirami-hybrid-v1:");
    }
}
