//! Phase 12 Research: zkML verification scaffold
//!
//! This module is a forward-compatible framework for adding cryptographic
//! proof-of-inference to Forge. The current `ProofVerifier` trait + a `mock`
//! backend let us wire proof verification into the ledger TODAY, even before
//! a real zkML backend (ezkl, risc0, halo2) is integrated.
//!
//! ## Intended integration path
//!
//! 1. A node running inference ALSO runs a zkML prover on the same inputs.
//! 2. The proof is attached to the `TradeRecord` via `ProofOfInference`.
//! 3. Consumers verifying the trade call `VerifierRegistry::verify(&proof)`.
//! 4. Failed verification causes the consumer to reject the trade (reputation
//!    penalty applied via the existing collusion / reputation pipeline).
//!
//! ## Why a mock backend
//!
//! Real zkML backends (ezkl, risc0) are heavyweight dependencies with
//! significant build-time and runtime costs. By shipping a `mock` backend
//! today, we can:
//!
//! - Test the integration end-to-end with no external deps
//! - Benchmark the overhead of the verification pipeline
//! - Ensure the `TradeRecord` / gossip layers gracefully handle proofs
//! - Onboard real backends later with zero API breakage
//!
//! ## Non-goals (Phase 12)
//!
//! - Actual SNARK/STARK circuits for GGUF inference
//! - Real ezkl / risc0 integration
//! - Performance optimization of proof generation
//! - Recursive proofs or aggregation
//!
//! Those are Phase 13+ concerns. This phase delivers the framework.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use tirami_core::NodeId;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the zkML verification layer.
#[derive(Debug, Error, PartialEq)]
pub enum ZkError {
    /// The `backend` field names a verifier that is not registered.
    #[error("unknown backend: {0}")]
    UnknownBackend(String),

    /// The verifier determined the proof is cryptographically invalid.
    #[error("proof verification failed: {0}")]
    VerificationFailed(String),

    /// The `backend_version` is not in the verifier's supported list.
    #[error("version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },

    /// The proof structure is internally inconsistent (e.g. empty token count).
    #[error("malformed proof: {0}")]
    MalformedProof(String),
}

// ---------------------------------------------------------------------------
// ProofOfInference data structure
// ---------------------------------------------------------------------------

/// A cryptographic commitment that a specific model produced a specific output
/// for a specific prompt, without revealing the model weights.
///
/// Backend-agnostic: the `backend` field identifies which zkML system
/// produced the proof (ezkl, risc0, halo2-custom, mock, etc.). The ledger
/// verifies via the corresponding backend registered in a [`VerifierRegistry`].
///
/// This struct is intentionally serializable so it can travel over gossip,
/// be stored in settlement statements, and be anchored to Bitcoin OP_RETURN.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProofOfInference {
    /// Node that ran the inference and produced this proof.
    pub prover: NodeId,

    /// Backend identifier: `"ezkl"`, `"risc0"`, `"halo2"`, `"mock"`, etc.
    pub backend: String,

    /// Backend version string (e.g. `"0.1.0"` for ezkl 0.1.x).
    pub backend_version: String,

    /// SHA-256 of the model file (GGUF or other format).
    pub model_hash: [u8; 32],

    /// SHA-256 of the input prompt bytes (before tokenization).
    pub prompt_hash: [u8; 32],

    /// SHA-256 of the output tokens concatenated as UTF-8 bytes.
    pub output_hash: [u8; 32],

    /// Number of tokens generated (must be > 0).
    pub token_count: u64,

    /// Unix timestamp of proof generation, milliseconds since epoch.
    pub generated_at_ms: u64,

    /// The opaque proof bytes (backend-specific serialization).
    ///
    /// For the `mock` backend this is `sha256(canonical_bytes())`.
    pub proof_bytes: Vec<u8>,

    /// Public inputs the verifier needs alongside `proof_bytes` (backend-specific).
    ///
    /// For the `mock` backend this is empty.
    pub public_inputs: Vec<u8>,
}

impl ProofOfInference {
    /// Canonical byte representation used for hashing and signing.
    ///
    /// Covers all semantically meaningful fields **except** `proof_bytes` and
    /// `public_inputs` (those are the proof itself, not the statement).
    /// The encoding is length-prefixed to avoid ambiguity between fields.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // prover node id (fixed 32 bytes)
        buf.extend_from_slice(&self.prover.0);

        // backend (length-prefixed utf-8)
        let b = self.backend.as_bytes();
        buf.extend_from_slice(&(b.len() as u32).to_le_bytes());
        buf.extend_from_slice(b);

        // backend_version (length-prefixed utf-8)
        let bv = self.backend_version.as_bytes();
        buf.extend_from_slice(&(bv.len() as u32).to_le_bytes());
        buf.extend_from_slice(bv);

        // fixed-size hashes
        buf.extend_from_slice(&self.model_hash);
        buf.extend_from_slice(&self.prompt_hash);
        buf.extend_from_slice(&self.output_hash);

        // counters (little-endian u64)
        buf.extend_from_slice(&self.token_count.to_le_bytes());
        buf.extend_from_slice(&self.generated_at_ms.to_le_bytes());

        buf
    }

    /// SHA-256 of `canonical_bytes()` — used as a dedup key for gossip and
    /// indexing (same role as `TradeRecord::dedup_key`).
    pub fn dedup_key(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(&self.canonical_bytes());
        h.finalize().into()
    }
}

// ---------------------------------------------------------------------------
// ProofVerifier trait
// ---------------------------------------------------------------------------

/// A pluggable backend for verifying [`ProofOfInference`] objects.
///
/// Implementations MUST be:
/// - **Deterministic**: same proof always produces the same result.
/// - **Side-effect-free**: no I/O, no network, no global state mutation.
/// - **Send + Sync**: safe to share across tokio tasks.
///
/// New backends (ezkl, risc0, halo2) are added by calling
/// [`VerifierRegistry::register`] — no changes to the trait or registry are
/// needed.
pub trait ProofVerifier: Send + Sync {
    /// The identifier that `ProofOfInference::backend` must match.
    fn name(&self) -> &str;

    /// Version strings this verifier accepts.
    fn supported_versions(&self) -> &[&str];

    /// Verify a proof.
    ///
    /// Returns `Ok(())` on success.  Returns a diagnostic [`ZkError`] on any
    /// failure — callers should treat all error variants as "proof rejected".
    fn verify(&self, proof: &ProofOfInference) -> Result<(), ZkError>;
}

// ---------------------------------------------------------------------------
// MockVerifier
// ---------------------------------------------------------------------------

/// A structurally-validating verifier for the `"mock"` backend.
///
/// Does **not** perform real cryptographic proving — it exists so we can test
/// the full verification pipeline today without a production zkML backend.
///
/// The mock proof contract is simple:
/// ```text
/// proof_bytes == sha256(canonical_bytes())
/// ```
/// A valid mock proof is produced by [`MockVerifier::build_mock_proof`].
pub struct MockVerifier;

impl ProofVerifier for MockVerifier {
    fn name(&self) -> &str {
        "mock"
    }

    fn supported_versions(&self) -> &[&str] {
        &["0.1.0"]
    }

    fn verify(&self, proof: &ProofOfInference) -> Result<(), ZkError> {
        // 1. Must be addressed to us.
        if proof.backend != "mock" {
            return Err(ZkError::UnknownBackend(proof.backend.clone()));
        }

        // 2. Version gate.
        if !self.supported_versions().contains(&proof.backend_version.as_str()) {
            return Err(ZkError::VersionMismatch {
                expected: "0.1.0".to_string(),
                actual: proof.backend_version.clone(),
            });
        }

        // 3. Structural invariants.
        if proof.token_count == 0 {
            return Err(ZkError::MalformedProof("token_count == 0".to_string()));
        }
        if proof.model_hash == [0u8; 32] {
            return Err(ZkError::MalformedProof("model_hash all zero".to_string()));
        }
        if proof.prompt_hash == [0u8; 32] {
            return Err(ZkError::MalformedProof("prompt_hash all zero".to_string()));
        }

        // 4. Mock cryptographic check: proof_bytes must equal sha256(canonical).
        let expected = {
            let mut h = Sha256::new();
            h.update(&proof.canonical_bytes());
            h.finalize().to_vec()
        };
        if proof.proof_bytes != expected {
            return Err(ZkError::VerificationFailed(
                "mock proof_bytes != sha256(canonical)".to_string(),
            ));
        }

        Ok(())
    }
}

impl MockVerifier {
    /// Build a structurally valid mock proof for testing.
    ///
    /// **Warning**: This is a test helper. Real zkML provers generate proof
    /// bytes using actual ZK circuits — they never call a method on the
    /// verifier to get the proof.  Only the `mock` backend exposes this
    /// shortcut.
    pub fn build_mock_proof(
        prover: NodeId,
        model_hash: [u8; 32],
        prompt_hash: [u8; 32],
        output_hash: [u8; 32],
        token_count: u64,
        generated_at_ms: u64,
    ) -> ProofOfInference {
        // Build the shell first (proof_bytes empty so canonical_bytes is stable).
        let mut proof = ProofOfInference {
            prover,
            backend: "mock".to_string(),
            backend_version: "0.1.0".to_string(),
            model_hash,
            prompt_hash,
            output_hash,
            token_count,
            generated_at_ms,
            proof_bytes: Vec::new(),
            public_inputs: Vec::new(),
        };

        // Compute proof_bytes = sha256(canonical_bytes()).
        let mut h = Sha256::new();
        h.update(&proof.canonical_bytes());
        proof.proof_bytes = h.finalize().to_vec();

        proof
    }
}

// ---------------------------------------------------------------------------
// VerifierRegistry
// ---------------------------------------------------------------------------

/// A dispatch table mapping backend names to [`ProofVerifier`] implementations.
///
/// Pre-registered backends: `"mock"`.
///
/// Future backends are added at startup via [`VerifierRegistry::register`];
/// the ledger / node integration code calls `registry.verify(&proof)` without
/// knowing which backend is in use.
pub struct VerifierRegistry {
    backends: HashMap<String, Box<dyn ProofVerifier>>,
}

impl VerifierRegistry {
    /// Create a new registry pre-populated with the `mock` backend.
    pub fn new() -> Self {
        let mut r = Self {
            backends: HashMap::new(),
        };
        r.register(Box::new(MockVerifier));
        r
    }

    /// Register a new backend verifier, replacing any existing one with the
    /// same name.
    pub fn register(&mut self, verifier: Box<dyn ProofVerifier>) {
        self.backends.insert(verifier.name().to_string(), verifier);
    }

    /// Verify a proof using the backend named in `proof.backend`.
    ///
    /// Returns `Err(ZkError::UnknownBackend)` if no verifier is registered for
    /// that backend, otherwise delegates to the registered verifier.
    pub fn verify(&self, proof: &ProofOfInference) -> Result<(), ZkError> {
        let Some(v) = self.backends.get(&proof.backend) else {
            return Err(ZkError::UnknownBackend(proof.backend.clone()));
        };
        v.verify(proof)
    }
}

impl Default for VerifierRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: a deterministic NodeId for tests.
    fn test_node() -> NodeId {
        NodeId([0x42u8; 32])
    }

    // Helper: non-zero hashes.
    fn model_hash() -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = 0xAB;
        h
    }
    fn prompt_hash() -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = 0xCD;
        h
    }
    fn output_hash() -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = 0xEF;
        h
    }

    // Helper: build a valid mock proof.
    fn valid_proof() -> ProofOfInference {
        MockVerifier::build_mock_proof(
            test_node(),
            model_hash(),
            prompt_hash(),
            output_hash(),
            42,
            1_700_000_000_000,
        )
    }

    // -----------------------------------------------------------------------

    #[test]
    fn test_mock_verifier_accepts_valid_proof() {
        let v = MockVerifier;
        let proof = valid_proof();
        assert!(v.verify(&proof).is_ok(), "valid mock proof must be accepted");
    }

    #[test]
    fn test_mock_verifier_rejects_wrong_backend() {
        let v = MockVerifier;
        let mut proof = valid_proof();
        proof.backend = "ezkl".to_string();
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::UnknownBackend(ref b) if b == "ezkl"),
            "wrong backend should yield UnknownBackend"
        );
    }

    #[test]
    fn test_mock_verifier_rejects_version_mismatch() {
        let v = MockVerifier;
        // Build a proof, then mutate version AFTER proof_bytes computed so
        // we exercise the version check before the crypto check.
        let mut proof = valid_proof();
        proof.backend_version = "9.9.9".to_string();
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::VersionMismatch { ref actual, .. } if actual == "9.9.9"),
            "wrong version should yield VersionMismatch"
        );
    }

    #[test]
    fn test_mock_verifier_rejects_zero_token_count() {
        let v = MockVerifier;
        // Manually build a proof with token_count=0 and correct proof_bytes.
        let mut proof = ProofOfInference {
            prover: test_node(),
            backend: "mock".to_string(),
            backend_version: "0.1.0".to_string(),
            model_hash: model_hash(),
            prompt_hash: prompt_hash(),
            output_hash: output_hash(),
            token_count: 0,
            generated_at_ms: 1_700_000_000_000,
            proof_bytes: Vec::new(),
            public_inputs: Vec::new(),
        };
        // Set correct proof_bytes so crypto check would pass — but token_count check comes first.
        let hash = {
            let mut h = Sha256::new();
            h.update(&proof.canonical_bytes());
            h.finalize().to_vec()
        };
        proof.proof_bytes = hash;
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::MalformedProof(ref m) if m.contains("token_count")),
            "zero token_count should yield MalformedProof"
        );
    }

    #[test]
    fn test_mock_verifier_rejects_all_zero_model_hash() {
        let v = MockVerifier;
        // Build then corrupt model_hash to all-zero; rebuild proof_bytes accordingly.
        let mut proof = ProofOfInference {
            prover: test_node(),
            backend: "mock".to_string(),
            backend_version: "0.1.0".to_string(),
            model_hash: [0u8; 32],
            prompt_hash: prompt_hash(),
            output_hash: output_hash(),
            token_count: 10,
            generated_at_ms: 1_700_000_000_000,
            proof_bytes: Vec::new(),
            public_inputs: Vec::new(),
        };
        let hash = {
            let mut h = Sha256::new();
            h.update(&proof.canonical_bytes());
            h.finalize().to_vec()
        };
        proof.proof_bytes = hash;
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::MalformedProof(ref m) if m.contains("model_hash")),
            "all-zero model_hash should yield MalformedProof"
        );
    }

    #[test]
    fn test_mock_verifier_rejects_all_zero_prompt_hash() {
        let v = MockVerifier;
        let mut proof = ProofOfInference {
            prover: test_node(),
            backend: "mock".to_string(),
            backend_version: "0.1.0".to_string(),
            model_hash: model_hash(),
            prompt_hash: [0u8; 32],
            output_hash: output_hash(),
            token_count: 10,
            generated_at_ms: 1_700_000_000_000,
            proof_bytes: Vec::new(),
            public_inputs: Vec::new(),
        };
        let hash = {
            let mut h = Sha256::new();
            h.update(&proof.canonical_bytes());
            h.finalize().to_vec()
        };
        proof.proof_bytes = hash;
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::MalformedProof(ref m) if m.contains("prompt_hash")),
            "all-zero prompt_hash should yield MalformedProof"
        );
    }

    #[test]
    fn test_mock_verifier_rejects_tampered_proof_bytes() {
        let v = MockVerifier;
        let mut proof = valid_proof();
        // Flip a byte in proof_bytes.
        if let Some(b) = proof.proof_bytes.first_mut() {
            *b = b.wrapping_add(1);
        }
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::VerificationFailed(_)),
            "tampered proof_bytes should yield VerificationFailed"
        );
    }

    #[test]
    fn test_verifier_registry_dispatches_by_backend() {
        let registry = VerifierRegistry::new();
        let proof = valid_proof();
        assert!(
            registry.verify(&proof).is_ok(),
            "registry should dispatch to MockVerifier and accept valid proof"
        );
    }

    #[test]
    fn test_verifier_registry_rejects_unknown_backend() {
        let registry = VerifierRegistry::new();
        let mut proof = valid_proof();
        proof.backend = "risc0".to_string();
        let err = registry.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::UnknownBackend(ref b) if b == "risc0"),
            "unregistered backend should yield UnknownBackend from registry"
        );
    }

    #[test]
    fn test_proof_canonical_bytes_deterministic() {
        let p1 = valid_proof();
        let p2 = valid_proof();
        assert_eq!(
            p1.canonical_bytes(),
            p2.canonical_bytes(),
            "canonical_bytes must be deterministic for identical proofs"
        );
    }

    #[test]
    fn test_proof_dedup_key_changes_with_any_field() {
        let base = valid_proof();
        let base_key = base.dedup_key();

        // Change token_count.
        let mut p = base.clone();
        p.token_count += 1;
        p.proof_bytes = Vec::new(); // proof_bytes not part of canonical
        assert_ne!(p.dedup_key(), base_key, "token_count change must alter dedup_key");

        // Change model_hash.
        let mut p = base.clone();
        p.model_hash[1] ^= 0xFF;
        assert_ne!(p.dedup_key(), base_key, "model_hash change must alter dedup_key");

        // Change prompt_hash.
        let mut p = base.clone();
        p.prompt_hash[0] ^= 0x01;
        assert_ne!(p.dedup_key(), base_key, "prompt_hash change must alter dedup_key");

        // Change prover.
        let mut p = base.clone();
        p.prover.0[0] ^= 0xFF;
        assert_ne!(p.dedup_key(), base_key, "prover change must alter dedup_key");

        // Change backend.
        let mut p = base.clone();
        p.backend = "other".to_string();
        assert_ne!(p.dedup_key(), base_key, "backend change must alter dedup_key");

        // Change generated_at_ms.
        let mut p = base.clone();
        p.generated_at_ms += 1;
        assert_ne!(
            p.dedup_key(),
            base_key,
            "generated_at_ms change must alter dedup_key"
        );
    }

    #[test]
    fn test_proof_dedup_key_ignores_proof_bytes_and_public_inputs() {
        // proof_bytes and public_inputs are NOT part of canonical_bytes, so
        // mutating them must NOT change the dedup_key.
        let mut p = valid_proof();
        let base_key = p.dedup_key();

        p.proof_bytes = vec![0xFF; 64];
        assert_eq!(
            p.dedup_key(),
            base_key,
            "proof_bytes must not affect dedup_key"
        );

        p.public_inputs = vec![1, 2, 3];
        assert_eq!(
            p.dedup_key(),
            base_key,
            "public_inputs must not affect dedup_key"
        );
    }

    // =========================================================================
    // Security tests: zkML proof forgery attacks
    // =========================================================================

    #[test]
    fn sec_mock_verifier_rejects_empty_proof_bytes() {
        // An empty proof_bytes vector is not sha256(canonical) and must fail.
        let v = MockVerifier;
        let mut proof = valid_proof();
        proof.proof_bytes = vec![];
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::VerificationFailed(_)),
            "empty proof_bytes must yield VerificationFailed, got {err:?}"
        );
    }

    #[test]
    fn sec_mock_verifier_rejects_single_byte_flip() {
        // A single-bit flip anywhere in proof_bytes must break verification.
        let v = MockVerifier;
        let mut proof = valid_proof();
        assert!(proof.proof_bytes.len() >= 2, "proof must have at least 2 bytes");
        // Flip byte at index 0.
        proof.proof_bytes[0] ^= 0x01;
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::VerificationFailed(_)),
            "single-byte flip must yield VerificationFailed"
        );
    }

    #[test]
    fn sec_mock_verifier_rejects_proof_for_different_model() {
        // Build a valid proof for model_hash A; change model_hash to B
        // and recompute valid proof_bytes for the new canonical → but
        // the original proof_bytes no longer matches.
        let v = MockVerifier;
        // Build valid proof for model A.
        let proof_for_a = valid_proof();
        // Now change model_hash to B; do NOT update proof_bytes.
        let mut proof_b = proof_for_a.clone();
        proof_b.model_hash = {
            let mut h = [0u8; 32];
            h[0] = 0x01; // different first byte
            h[1] = 0x23;
            h
        };
        // proof_bytes was computed for model A — it no longer matches canonical for model B.
        let err = v.verify(&proof_b).unwrap_err();
        assert!(
            matches!(err, ZkError::VerificationFailed(_)),
            "proof for model A must not verify against model B canonical: {err:?}"
        );
    }

    #[test]
    fn sec_mock_verifier_rejects_proof_with_all_zero_proof_bytes() {
        // 32 zero bytes is not sha256(anything_reasonable) for a valid proof.
        let v = MockVerifier;
        let mut proof = valid_proof();
        proof.proof_bytes = vec![0u8; 32];
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::VerificationFailed(_)),
            "32 zero bytes must not pass as valid proof_bytes: {err:?}"
        );
    }

    #[test]
    fn sec_verifier_registry_rejects_unregistered_backend_fake() {
        // "fake-backend" is not registered → must return UnknownBackend.
        let registry = VerifierRegistry::new();
        let mut proof = valid_proof();
        proof.backend = "fake-backend".to_string();
        let err = registry.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::UnknownBackend(ref b) if b == "fake-backend"),
            "unregistered 'fake-backend' must yield UnknownBackend, got {err:?}"
        );
    }

    #[test]
    fn sec_verifier_registry_rejects_empty_backend_string() {
        // An empty backend string must also fail with UnknownBackend.
        let registry = VerifierRegistry::new();
        let mut proof = valid_proof();
        proof.backend = String::new();
        let err = registry.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::UnknownBackend(ref b) if b.is_empty()),
            "empty backend string must yield UnknownBackend, got {err:?}"
        );
    }

    #[test]
    fn sec_mock_verifier_rejects_tampered_prover_field() {
        // Change only the prover field after proof_bytes is computed
        // → canonical_bytes differ → sha256 mismatch → VerificationFailed.
        let v = MockVerifier;
        let mut proof = valid_proof();
        proof.prover = NodeId([0xDEu8; 32]);
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::VerificationFailed(_)),
            "tampered prover must cause proof_bytes mismatch, got {err:?}"
        );
    }

    #[test]
    fn sec_mock_verifier_rejects_tampered_generated_at_ms() {
        // Change generated_at_ms → canonical bytes change → proof_bytes stale.
        let v = MockVerifier;
        let mut proof = valid_proof();
        proof.generated_at_ms += 1;
        let err = v.verify(&proof).unwrap_err();
        assert!(
            matches!(err, ZkError::VerificationFailed(_)),
            "tampered generated_at_ms must cause VerificationFailed, got {err:?}"
        );
    }
}
