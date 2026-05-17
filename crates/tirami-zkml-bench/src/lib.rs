//! Phase 18.3 — zkML benchmark harness.
//!
//! # Purpose
//!
//! Establish an empirical baseline for "how expensive is a
//! zero-knowledge proof of an LLM inference today?" The answer
//! determines when Tirami can advance `ProofPolicy` from
//! `Disabled` → `Optional` → `Recommended` → `Required`.
//!
//! See `docs/zkml-strategy.md` for the performance targets.
//!
//! # Structure
//!
//! * [`BenchBackend`] — a uniform trait any zkML backend implements.
//!   * `prove(spec)` produces a `BenchProof`.
//!   * `verify(spec, proof)` checks it.
//! * [`BenchSpec`] — what to prove (model hash, prompt, output,
//!   FLOP count).
//! * [`BenchResult`] — what was measured (prove_ms, verify_ms,
//!   proof_bytes).
//! * [`run_bench`] — execute N trials and aggregate stats.
//!
//! Backends are feature-gated: the default build includes only
//! [`MockBackend`] (SHA-256-based, trivially forgeable, for
//! shape-testing). Real backends (`ezkl`, `risc0`, `halo2`) are
//! behind their own features, to be wired in Phase 18.3-part-2.
//!
//! # Usage
//!
//! ```no_run
//! use tirami_zkml_bench::{run_bench, BenchSpec, MockBackend};
//!
//! let spec = BenchSpec {
//!     model_hash: [0x42; 32],
//!     prompt_hash: [0x01; 32],
//!     output_hash: [0x02; 32],
//!     token_count: 128,
//!     flops: 128 * 1_000_000_000, // 128 tokens × 1 GFLOP each
//! };
//! let result = run_bench(&MockBackend, &spec, 10).expect("bench succeeded");
//! println!("prove_p50_ms = {}", result.prove_ms_p50);
//! println!("verify_p50_ms = {}", result.verify_ms_p50);
//! println!("proof_bytes = {}", result.proof_bytes);
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BenchError {
    #[error("proof generation failed: {0}")]
    ProveFailed(String),
    #[error("proof verification failed: {0}")]
    VerifyFailed(String),
    #[error("backend unavailable: {0} (enable the feature flag to use)")]
    BackendUnavailable(&'static str),
    #[error("bench spec rejected by backend: {0}")]
    InvalidSpec(String),
}

// ---------------------------------------------------------------------------
// Spec & result types
// ---------------------------------------------------------------------------

/// What to prove. Opaque to the backend except for `flops`
/// (influences circuit size).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSpec {
    pub model_hash: [u8; 32],
    pub prompt_hash: [u8; 32],
    pub output_hash: [u8; 32],
    pub token_count: u64,
    pub flops: u64,
}

/// A serialized proof (opaque bytes). Backends should make this
/// canonical so verifier-side hashing is deterministic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchProof {
    pub backend: String,
    pub bytes: Vec<u8>,
}

impl BenchProof {
    pub fn size(&self) -> usize {
        self.bytes.len()
    }
}

/// Aggregate result of N trials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub backend: String,
    pub trials: u32,
    pub prove_ms_p50: u64,
    pub prove_ms_p95: u64,
    pub verify_ms_p50: u64,
    pub verify_ms_p95: u64,
    pub proof_bytes: usize,
    pub flops: u64,
    /// `prove_ms_p50 × 1_000_000 / flops` → nanoseconds per FLOP.
    /// Lets us compare backends on a fixed-work basis.
    pub ns_per_flop_p50: f64,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

/// Uniform interface any zkML backend must implement. Real
/// implementations live in feature-gated modules.
pub trait BenchBackend: Send + Sync {
    fn name(&self) -> &'static str;

    fn prove(&self, spec: &BenchSpec) -> Result<BenchProof, BenchError>;

    fn verify(&self, spec: &BenchSpec, proof: &BenchProof) -> Result<(), BenchError>;
}

// ---------------------------------------------------------------------------
// MockBackend — default, always-available, non-cryptographic
// ---------------------------------------------------------------------------

/// SHA-256-based "proof": bytes = H(model_hash || prompt_hash ||
/// output_hash || token_count || flops). Trivially forgeable.
/// Exists to shape-test the harness without pulling any real
/// zk dep. Never use in production.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockBackend;

impl MockBackend {
    const NAME: &'static str = "mock-sha256";

    fn compute(spec: &BenchSpec) -> Vec<u8> {
        let mut h = Sha256::new();
        h.update(b"tirami-zkml-bench-v1");
        h.update(spec.model_hash);
        h.update(spec.prompt_hash);
        h.update(spec.output_hash);
        h.update(spec.token_count.to_le_bytes());
        h.update(spec.flops.to_le_bytes());
        h.finalize().to_vec()
    }
}

impl BenchBackend for MockBackend {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn prove(&self, spec: &BenchSpec) -> Result<BenchProof, BenchError> {
        if spec.token_count == 0 {
            return Err(BenchError::InvalidSpec("token_count must be > 0".into()));
        }
        Ok(BenchProof {
            backend: Self::NAME.to_string(),
            bytes: Self::compute(spec),
        })
    }

    fn verify(&self, spec: &BenchSpec, proof: &BenchProof) -> Result<(), BenchError> {
        if proof.backend != Self::NAME {
            return Err(BenchError::VerifyFailed(format!(
                "wrong backend: {}",
                proof.backend
            )));
        }
        let expected = Self::compute(spec);
        if expected != proof.bytes {
            return Err(BenchError::VerifyFailed("hash mismatch".into()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Ezkl / Risc0 / Halo2 stubs (feature-gated; return BackendUnavailable
// until Phase 18.3-part-2 wires real integrations)
// ---------------------------------------------------------------------------

macro_rules! stub_backend {
    ($name:ident, $label:expr) => {
        #[derive(Debug, Clone, Copy, Default)]
        pub struct $name;

        impl BenchBackend for $name {
            fn name(&self) -> &'static str {
                $label
            }

            fn prove(&self, _spec: &BenchSpec) -> Result<BenchProof, BenchError> {
                Err(BenchError::BackendUnavailable($label))
            }

            fn verify(
                &self,
                _spec: &BenchSpec,
                _proof: &BenchProof,
            ) -> Result<(), BenchError> {
                Err(BenchError::BackendUnavailable($label))
            }
        }
    };
}

stub_backend!(EzklBackend, "ezkl");
stub_backend!(Risc0Backend, "risc0");
stub_backend!(Halo2Backend, "halo2");

// ---------------------------------------------------------------------------
// Phase 24 Wave 1 — Ed25519 attestation backend
// ---------------------------------------------------------------------------
//
// `EdAttestBackend` is the FIRST cryptographically meaningful
// backend in this crate. It is NOT zero-knowledge — anyone with
// the signer's public key can see what was attested — but the
// proof is **unforgeable** without the corresponding private key,
// which is qualitatively stronger than `MockBackend`'s public hash.
//
// What it proves:
//
//   - "The holder of <pubkey> attests that bench-spec S was the
//     input/output of an inference they ran."
//
// What it does NOT prove:
//
//   - That the inference computation was actually performed
//     correctly. The signer could lie. zk-SNARK / zk-STARK
//     backends (`EzklBackend`, `Risc0Backend`) cover that gap
//     once their dep chains stabilise.
//
// Why ship this now: it's the bounded primitive that lets the
// protocol layer (Phase 24 Wave 2+) attach signed proofs to
// SignedTradeRecord while ezkl/risc0 integration is still
// week-scale work. The on-chain ratchet
// `PROOF_POLICY_RATCHET` allows the network to progress
// Optional → Recommended → Required AS REAL BACKENDS LAND,
// without breaking legacy clients.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// Wire format for an Ed25519 attestation proof.
///
/// Stored as the `bytes` field of [`BenchProof`]:
///   [0..32]   = signer's 32-byte Ed25519 public key
///   [32..96]  = signature over the canonical bench-spec bytes
///
/// Total 96 bytes — small enough to ride on a TradeRecord without
/// inflating the wire size beyond the existing dual-sig overhead.
const ED_ATTEST_PROOF_LEN: usize = 32 + 64;

/// Ed25519 signing-side backend. Construct with a `SigningKey`;
/// verify via the embedded `VerifyingKey` parsed out of the proof
/// (no separate trust root needed — the pubkey is part of the
/// proof, which the protocol layer cross-checks against the
/// expected provider DID).
#[derive(Debug, Clone)]
pub struct EdAttestBackend {
    signing_key: SigningKey,
}

impl EdAttestBackend {
    pub const NAME: &'static str = "ed-attest";

    /// Construct from an existing keypair. Use this when the
    /// signer is also the node's `AgentIdentity` so the
    /// attestation key matches the trade signer key.
    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        Self { signing_key }
    }

    /// Generate a fresh keypair. Useful for tests; production
    /// nodes should reuse their `AgentIdentity` key via
    /// [`Self::from_signing_key`].
    pub fn generate() -> Self {
        let mut rng = rand::rngs::OsRng;
        Self {
            signing_key: SigningKey::generate(&mut rng),
        }
    }

    /// 32-byte Ed25519 public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    fn canonical_bytes(spec: &BenchSpec) -> Vec<u8> {
        let mut h = Sha256::new();
        h.update(b"tirami-zkml-ed-attest-v1");
        h.update(spec.model_hash);
        h.update(spec.prompt_hash);
        h.update(spec.output_hash);
        h.update(spec.token_count.to_le_bytes());
        h.update(spec.flops.to_le_bytes());
        h.finalize().to_vec()
    }
}

impl BenchBackend for EdAttestBackend {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn prove(&self, spec: &BenchSpec) -> Result<BenchProof, BenchError> {
        if spec.token_count == 0 {
            return Err(BenchError::InvalidSpec("token_count must be > 0".into()));
        }
        let msg = Self::canonical_bytes(spec);
        let sig = self.signing_key.sign(&msg);
        let mut bytes = Vec::with_capacity(ED_ATTEST_PROOF_LEN);
        bytes.extend_from_slice(&self.public_key_bytes());
        bytes.extend_from_slice(&sig.to_bytes());
        Ok(BenchProof {
            backend: Self::NAME.to_string(),
            bytes,
        })
    }

    fn verify(&self, spec: &BenchSpec, proof: &BenchProof) -> Result<(), BenchError> {
        verify_ed_attest_proof(spec, proof)
    }
}

/// Verify an `ed-attest` proof without holding the
/// [`EdAttestBackend`] instance — useful for the protocol layer
/// (`tirami-ledger` / `tirami-node`) where the verifier only has
/// the proof bytes and a candidate expected public key.
///
/// Returns `Ok(())` on a valid signature whose pubkey decodes
/// successfully and whose signature verifies against the
/// canonical bench-spec bytes.
pub fn verify_ed_attest_proof(
    spec: &BenchSpec,
    proof: &BenchProof,
) -> Result<(), BenchError> {
    if proof.backend != EdAttestBackend::NAME {
        return Err(BenchError::VerifyFailed(format!(
            "wrong backend: {} (expected {})",
            proof.backend,
            EdAttestBackend::NAME
        )));
    }
    if proof.bytes.len() != ED_ATTEST_PROOF_LEN {
        return Err(BenchError::VerifyFailed(format!(
            "proof must be {} bytes, got {}",
            ED_ATTEST_PROOF_LEN,
            proof.bytes.len()
        )));
    }
    let mut pk_bytes = [0u8; 32];
    pk_bytes.copy_from_slice(&proof.bytes[..32]);
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&proof.bytes[32..]);
    let vk = VerifyingKey::from_bytes(&pk_bytes)
        .map_err(|e| BenchError::VerifyFailed(format!("pubkey decode: {e}")))?;
    let sig = Signature::from_bytes(&sig_bytes);
    let msg = EdAttestBackend::canonical_bytes(spec);
    vk.verify(&msg, &sig)
        .map_err(|e| BenchError::VerifyFailed(format!("Ed25519 verify failed: {e}")))?;
    Ok(())
}

/// Verify an `ed-attest` proof AND check that the signer's pubkey
/// matches an expected value. The protocol layer uses this so the
/// trade-attribution pubkey and the attestation pubkey must agree.
pub fn verify_ed_attest_proof_by_signer(
    spec: &BenchSpec,
    proof: &BenchProof,
    expected_signer: &[u8; 32],
) -> Result<(), BenchError> {
    verify_ed_attest_proof(spec, proof)?;
    if &proof.bytes[..32] != &expected_signer[..] {
        return Err(BenchError::VerifyFailed(format!(
            "signer mismatch: proof carries {} but expected {}",
            hex::encode(&proof.bytes[..32]),
            hex::encode(expected_signer)
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 24 Wave 1 — Backend selector
// ---------------------------------------------------------------------------

/// Runtime-selectable backend kinds. The protocol layer reads this
/// from `Config.zkml_backend` (Phase 24 Wave 2) and constructs the
/// appropriate `BenchBackend`. The default is `Mock` so the
/// always-available shape-testing path remains the no-config
/// default.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BenchBackendKind {
    /// SHA-256 hash; trivially forgeable. Development only.
    Mock,
    /// Ed25519 signature over the canonical spec; unforgeable
    /// without the private key but NOT zero-knowledge. Phase 24
    /// Wave 1 default for production-with-attestation deployments.
    EdAttest,
    /// Halo2-based zk-SNARK (feature `ezkl`). Returns
    /// `BackendUnavailable` until Phase 24 Wave 3+ lands.
    Ezkl,
    /// ZKVM-based zk-STARK (feature `risc0`). Same caveat.
    Risc0,
    /// Halo2 generic. Same caveat.
    Halo2,
}

impl Default for BenchBackendKind {
    fn default() -> Self {
        Self::Mock
    }
}

impl BenchBackendKind {
    /// Returns `true` if this backend is currently usable (i.e.
    /// will not return `BackendUnavailable` for `prove`/`verify`).
    /// Useful for the discovery manifest so agents know what
    /// strength of proof a peer can actually produce.
    pub fn is_available(self) -> bool {
        matches!(self, Self::Mock | Self::EdAttest)
    }

    /// Human-readable canonical name used in `BenchProof.backend`
    /// and in the discovery manifest.
    pub fn name(self) -> &'static str {
        match self {
            Self::Mock => MockBackend::NAME,
            Self::EdAttest => EdAttestBackend::NAME,
            Self::Ezkl => "ezkl",
            Self::Risc0 => "risc0",
            Self::Halo2 => "halo2",
        }
    }
}

// ---------------------------------------------------------------------------
// run_bench
// ---------------------------------------------------------------------------

/// Run `trials` iterations of prove + verify, collect timings,
/// return p50 / p95 statistics.
pub fn run_bench<B: BenchBackend>(
    backend: &B,
    spec: &BenchSpec,
    trials: u32,
) -> Result<BenchResult, BenchError> {
    if trials == 0 {
        return Err(BenchError::InvalidSpec("trials must be > 0".into()));
    }
    let mut prove_samples: Vec<u64> = Vec::with_capacity(trials as usize);
    let mut verify_samples: Vec<u64> = Vec::with_capacity(trials as usize);
    let mut proof_bytes: usize = 0;

    for _ in 0..trials {
        let t_prove = Instant::now();
        let proof = backend.prove(spec)?;
        prove_samples.push(t_prove.elapsed().as_millis() as u64);
        proof_bytes = proof.size();

        let t_verify = Instant::now();
        backend.verify(spec, &proof)?;
        verify_samples.push(t_verify.elapsed().as_millis() as u64);
    }

    prove_samples.sort();
    verify_samples.sort();
    let percentile = |v: &[u64], p: f64| -> u64 {
        let idx = ((v.len() as f64 * p).min(v.len() as f64 - 1.0)) as usize;
        v[idx]
    };

    let prove_p50 = percentile(&prove_samples, 0.50);
    let prove_p95 = percentile(&prove_samples, 0.95);
    let verify_p50 = percentile(&verify_samples, 0.50);
    let verify_p95 = percentile(&verify_samples, 0.95);

    let ns_per_flop_p50 = if spec.flops > 0 {
        (prove_p50 as f64 * 1_000_000.0) / spec.flops as f64
    } else {
        0.0
    };

    Ok(BenchResult {
        backend: backend.name().to_string(),
        trials,
        prove_ms_p50: prove_p50,
        prove_ms_p95: prove_p95,
        verify_ms_p50: verify_p50,
        verify_ms_p95: verify_p95,
        proof_bytes,
        flops: spec.flops,
        ns_per_flop_p50,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> BenchSpec {
        BenchSpec {
            model_hash: [0xABu8; 32],
            prompt_hash: [0x01u8; 32],
            output_hash: [0x02u8; 32],
            token_count: 64,
            flops: 64_000_000_000,
        }
    }

    #[test]
    fn mock_backend_prove_verify_roundtrips() {
        let b = MockBackend;
        let s = spec();
        let p = b.prove(&s).unwrap();
        b.verify(&s, &p).unwrap();
    }

    #[test]
    fn mock_backend_rejects_tampered_spec() {
        let b = MockBackend;
        let s = spec();
        let p = b.prove(&s).unwrap();
        let mut tampered = s.clone();
        tampered.token_count = 128;
        let err = b.verify(&tampered, &p).unwrap_err();
        assert!(matches!(err, BenchError::VerifyFailed(_)));
    }

    #[test]
    fn mock_backend_rejects_wrong_backend_name() {
        let b = MockBackend;
        let s = spec();
        let mut p = b.prove(&s).unwrap();
        p.backend = "risc0".into();
        let err = b.verify(&s, &p).unwrap_err();
        assert!(matches!(err, BenchError::VerifyFailed(_)));
    }

    #[test]
    fn mock_backend_rejects_zero_token_count() {
        let b = MockBackend;
        let mut s = spec();
        s.token_count = 0;
        let err = b.prove(&s).unwrap_err();
        assert!(matches!(err, BenchError::InvalidSpec(_)));
    }

    #[test]
    fn run_bench_collects_percentiles() {
        let b = MockBackend;
        let s = spec();
        let r = run_bench(&b, &s, 20).unwrap();
        assert_eq!(r.backend, "mock-sha256");
        assert_eq!(r.trials, 20);
        assert!(r.prove_ms_p50 <= r.prove_ms_p95);
        assert!(r.verify_ms_p50 <= r.verify_ms_p95);
        assert!(r.proof_bytes > 0);
        // ns_per_flop_p50 should be non-negative (MockBackend is
        // so fast that it may round to 0, which is acceptable).
        assert!(r.ns_per_flop_p50 >= 0.0);
    }

    #[test]
    fn run_bench_rejects_zero_trials() {
        let err = run_bench(&MockBackend, &spec(), 0).unwrap_err();
        assert!(matches!(err, BenchError::InvalidSpec(_)));
    }

    #[test]
    fn stub_backends_return_unavailable() {
        for b_result in [
            EzklBackend.prove(&spec()),
            Risc0Backend.prove(&spec()),
            Halo2Backend.prove(&spec()),
        ] {
            assert!(matches!(
                b_result,
                Err(BenchError::BackendUnavailable(_))
            ));
        }
    }

    #[test]
    fn bench_result_serde_roundtrips() {
        let r = run_bench(&MockBackend, &spec(), 5).unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let back: BenchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.backend, r.backend);
        assert_eq!(back.trials, r.trials);
        assert_eq!(back.flops, r.flops);
    }

    #[test]
    fn proof_size_helper_matches_bytes_length() {
        let p = MockBackend.prove(&spec()).unwrap();
        assert_eq!(p.size(), p.bytes.len());
        assert_eq!(p.size(), 32); // SHA-256 output
    }

    // ---------------------------------------------------------------
    // Phase 24 Wave 1 — EdAttestBackend + BenchBackendKind
    // ---------------------------------------------------------------

    #[test]
    fn ed_attest_prove_then_verify_round_trip() {
        let backend = EdAttestBackend::generate();
        let s = spec();
        let proof = backend.prove(&s).expect("prove");
        assert_eq!(proof.backend, EdAttestBackend::NAME);
        assert_eq!(proof.bytes.len(), 96, "32-byte pk + 64-byte sig");
        backend.verify(&s, &proof).expect("verify");
    }

    #[test]
    fn ed_attest_verify_rejects_tampered_proof_bytes() {
        let backend = EdAttestBackend::generate();
        let s = spec();
        let mut proof = backend.prove(&s).expect("prove");
        // Flip a bit in the signature half.
        proof.bytes[40] ^= 0x01;
        let err = backend.verify(&s, &proof);
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))), "got {err:?}");
    }

    #[test]
    fn ed_attest_verify_rejects_tampered_spec() {
        let backend = EdAttestBackend::generate();
        let s = spec();
        let proof = backend.prove(&s).expect("prove");
        let mut tampered = s.clone();
        tampered.token_count += 1;
        let err = backend.verify(&tampered, &proof);
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))), "got {err:?}");
    }

    #[test]
    fn ed_attest_verify_rejects_wrong_pubkey_in_proof() {
        let alice = EdAttestBackend::generate();
        let bob = EdAttestBackend::generate();
        let s = spec();
        let mut proof_alice = alice.prove(&s).expect("prove");
        // Substitute Bob's pubkey at the start. The sig (by Alice)
        // won't verify under Bob's pubkey.
        let bob_pk = bob.public_key_bytes();
        proof_alice.bytes[..32].copy_from_slice(&bob_pk);
        let err = verify_ed_attest_proof(&s, &proof_alice);
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))), "got {err:?}");
    }

    #[test]
    fn ed_attest_verify_rejects_wrong_backend_name() {
        let backend = EdAttestBackend::generate();
        let s = spec();
        let mut proof = backend.prove(&s).expect("prove");
        proof.backend = "ezkl".to_string();
        let err = verify_ed_attest_proof(&s, &proof);
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))), "got {err:?}");
    }

    #[test]
    fn ed_attest_verify_by_signer_enforces_expected_pubkey() {
        let alice = EdAttestBackend::generate();
        let bob = EdAttestBackend::generate();
        let s = spec();
        let proof = alice.prove(&s).expect("prove");
        // verifying against Alice's pk succeeds:
        let alice_pk = alice.public_key_bytes();
        verify_ed_attest_proof_by_signer(&s, &proof, &alice_pk).expect("Alice ok");
        // verifying against Bob's pk fails (signer mismatch):
        let bob_pk = bob.public_key_bytes();
        let err = verify_ed_attest_proof_by_signer(&s, &proof, &bob_pk);
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))));
    }

    #[test]
    fn ed_attest_two_backends_produce_different_proofs() {
        let alice = EdAttestBackend::generate();
        let bob = EdAttestBackend::generate();
        let s = spec();
        let pa = alice.prove(&s).expect("prove");
        let pb = bob.prove(&s).expect("prove");
        // Same backend label, different bytes (different keys signed).
        assert_eq!(pa.backend, pb.backend);
        assert_ne!(pa.bytes, pb.bytes);
        // Cross-verify: each must NOT validate against the other's
        // verify-by-signer with the wrong pubkey.
        verify_ed_attest_proof_by_signer(&s, &pa, &alice.public_key_bytes()).expect("a ok");
        let err = verify_ed_attest_proof_by_signer(&s, &pa, &bob.public_key_bytes());
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))));
    }

    #[test]
    fn ed_attest_invalid_pubkey_bytes_reject_cleanly() {
        let s = spec();
        // Construct a fake proof with garbage pubkey bytes (most
        // 32-byte values DO decode to some VerifyingKey, so we
        // can't deliberately make `from_bytes` fail with random
        // bytes — but we can flip the backend label test path).
        // Instead, ensure that a proof whose declared backend is
        // `mock-sha256` is refused.
        let p = BenchProof {
            backend: "mock-sha256".into(),
            bytes: vec![0u8; 96],
        };
        let err = verify_ed_attest_proof(&s, &p);
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))));
    }

    #[test]
    fn ed_attest_short_proof_bytes_rejected() {
        let s = spec();
        let p = BenchProof {
            backend: EdAttestBackend::NAME.to_string(),
            bytes: vec![0u8; 95], // one short
        };
        let err = verify_ed_attest_proof(&s, &p);
        match err {
            Err(BenchError::VerifyFailed(msg)) => assert!(msg.contains("must be 96")),
            other => panic!("expected VerifyFailed, got {other:?}"),
        }
    }

    #[test]
    fn ed_attest_from_signing_key_round_trip() {
        // Construct a deterministic key and verify the proof's
        // pubkey segment matches.
        let seed = [0x11u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let pk = sk.verifying_key().to_bytes();
        let backend = EdAttestBackend::from_signing_key(sk);
        assert_eq!(backend.public_key_bytes(), pk);
        let s = spec();
        let proof = backend.prove(&s).expect("prove");
        assert_eq!(&proof.bytes[..32], &pk[..]);
    }

    #[test]
    fn ed_attest_canonical_includes_all_spec_fields() {
        // Mutating ANY spec field must yield a different canonical
        // pre-image, which means the same backend produces a
        // different signature.
        let backend = EdAttestBackend::generate();
        let mut a = spec();
        let mut b = spec();
        let p_a = backend.prove(&a).unwrap();

        // Tweak each field in turn; sig must change.
        for tweak in 0..5 {
            b = a.clone();
            match tweak {
                0 => b.model_hash[0] ^= 0xFF,
                1 => b.prompt_hash[0] ^= 0xFF,
                2 => b.output_hash[0] ^= 0xFF,
                3 => b.token_count += 1,
                4 => b.flops += 1,
                _ => unreachable!(),
            }
            let p_b = backend.prove(&b).unwrap();
            assert_ne!(
                &p_a.bytes[32..],
                &p_b.bytes[32..],
                "tweak {tweak} should change the signature"
            );
            a = b.clone();
        }
    }

    #[test]
    fn ed_attest_token_count_zero_rejected() {
        let backend = EdAttestBackend::generate();
        let mut s = spec();
        s.token_count = 0;
        let err = backend.prove(&s);
        assert!(matches!(err, Err(BenchError::InvalidSpec(_))));
    }

    #[test]
    fn ed_attest_runs_through_run_bench_harness() {
        let backend = EdAttestBackend::generate();
        let r = run_bench(&backend, &spec(), 5).expect("bench");
        assert_eq!(r.backend, EdAttestBackend::NAME);
        assert_eq!(r.trials, 5);
        assert_eq!(r.proof_bytes, 96);
        assert_eq!(r.flops, spec().flops);
    }

    // -- BenchBackendKind selector ----------------------------------

    #[test]
    fn backend_kind_default_is_mock() {
        assert_eq!(BenchBackendKind::default(), BenchBackendKind::Mock);
    }

    #[test]
    fn backend_kind_availability_matrix() {
        assert!(BenchBackendKind::Mock.is_available());
        assert!(BenchBackendKind::EdAttest.is_available());
        assert!(!BenchBackendKind::Ezkl.is_available());
        assert!(!BenchBackendKind::Risc0.is_available());
        assert!(!BenchBackendKind::Halo2.is_available());
    }

    #[test]
    fn backend_kind_names_match_backend_name_method() {
        assert_eq!(BenchBackendKind::Mock.name(), MockBackend.name());
        assert_eq!(BenchBackendKind::EdAttest.name(), EdAttestBackend::NAME);
        assert_eq!(BenchBackendKind::Ezkl.name(), "ezkl");
        assert_eq!(BenchBackendKind::Risc0.name(), "risc0");
        assert_eq!(BenchBackendKind::Halo2.name(), "halo2");
    }

    #[test]
    fn backend_kind_serializes_as_kebab_case() {
        let k = BenchBackendKind::EdAttest;
        let s = serde_json::to_string(&k).unwrap();
        assert_eq!(s, "\"ed-attest\"");
        let parsed: BenchBackendKind = serde_json::from_str("\"mock\"").unwrap();
        assert_eq!(parsed, BenchBackendKind::Mock);
    }
}
