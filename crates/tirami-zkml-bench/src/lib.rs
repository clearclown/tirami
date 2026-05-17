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

/// Phase 24 Wave 5.1 — machine-readable taxonomy of how strong a
/// backend's proof is. Agents read this off the discovery manifest
/// (via `BenchBackendKind::strength()`) and prefer
/// `ComputeBound > InputOutputBound > Cryptographic > None`.
///
/// The ordering is monotonic in cryptographic guarantee. A backend
/// whose proof is `ComputeBound` proves strictly more than one whose
/// proof is `InputOutputBound`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendStrength {
    /// Anyone can recompute the proof bytes. No cryptographic
    /// claim. Dev/shape-testing only (`MockBackend`, scaffolds).
    None,
    /// Unforgeable without the signer's private key, but the proof
    /// asserts only identity, not correctness of computation
    /// (`EdAttestBackend`).
    Cryptographic,
    /// zk-SNARK or zk-STARK over the commitment to input/output
    /// pairs. The proof binds the trade to specific `BenchSpec`
    /// values cryptographically — a prover who didn't see the
    /// (input, output) pair can't forge it. Does NOT verify that
    /// the *computation* producing the output was correct (the
    /// prover could lie about output). Wave 5.1+ landing target
    /// for the real `Risc0Backend` / `EzklBackend`.
    InputOutputBound,
    /// zk-SNARK over the computation itself: the circuit runs
    /// (a quantised approximation of) the model's forward pass
    /// and commits to the output. A passing proof guarantees the
    /// claimed output is what that exact model would have produced
    /// on the claimed input. Research scope (Wave 5.3+).
    ComputeBound,
}

impl BackendStrength {
    /// Short kebab-case tag — what gets advertised on the
    /// protocol feature vector / discovery manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Cryptographic => "cryptographic",
            Self::InputOutputBound => "input-output-bound",
            Self::ComputeBound => "compute-bound",
        }
    }
}

/// Uniform interface any zkML backend must implement. Real
/// implementations live in feature-gated modules.
pub trait BenchBackend: Send + Sync {
    fn name(&self) -> &'static str;

    fn prove(&self, spec: &BenchSpec) -> Result<BenchProof, BenchError>;

    fn verify(&self, spec: &BenchSpec, proof: &BenchProof) -> Result<(), BenchError>;

    /// Phase 24 Wave 5.1 — what kind of guarantee does this
    /// backend's proof provide? Default `None` so legacy / test
    /// backends without a categorisation are conservatively
    /// classed as "no cryptographic claim".
    fn strength(&self) -> BackendStrength {
        BackendStrength::None
    }
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
// Ezkl / Risc0 / Halo2 — feature-gated.
//
// Default build (no features): return `BackendUnavailable` so the
// harness compiles + tests stay lean.
//
// Phase 24 Wave 5.0 — when the corresponding feature is enabled,
// the backend produces a *scaffold* commitment that:
//   - is deterministic (same spec → same proof bytes)
//   - has a backend-specific version prefix in the canonical pre-image
//   - verifies via recomputation (NOT zero-knowledge; not yet zk-SNARK)
//
// The scaffold establishes the wire format and lets downstream code
// (`SignedTradeRecord.attestation`, gossip verifier) exercise the
// non-ed-attest path. Real risc0-zkvm / ezkl / halo2 crate imports
// land in Wave 5.1+ (see `docs/phase-24-wave-5-zk-backends.md`).
// ---------------------------------------------------------------------------

/// Phase 24 Wave 5.0 — compute the scaffold commitment for a given
/// backend label + spec. The label is part of the canonical pre-image
/// so collisions across backends are structurally impossible. Format
/// (versioned, see `SCAFFOLD_VERSION`): 32-byte SHA-256 over
/// `b"tirami-{label}-scaffold-v0" || canonical_spec`.
fn scaffold_commitment(label: &str, spec: &BenchSpec) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"tirami-");
    h.update(label.as_bytes());
    h.update(b"-scaffold-v0");
    h.update(spec.model_hash);
    h.update(spec.prompt_hash);
    h.update(spec.output_hash);
    h.update(spec.token_count.to_le_bytes());
    h.update(spec.flops.to_le_bytes());
    h.finalize().into()
}

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

#[cfg(not(feature = "ezkl"))]
stub_backend!(EzklBackend, "ezkl");

#[cfg(not(feature = "risc0"))]
stub_backend!(Risc0Backend, "risc0");

#[cfg(not(feature = "halo2"))]
stub_backend!(Halo2Backend, "halo2");

macro_rules! scaffold_backend {
    ($name:ident, $label:expr) => {
        /// Phase 24 Wave 5.0 — scaffold backend. Produces a
        /// deterministic 32-byte SHA-256 commitment keyed by the
        /// backend label and the full canonical `BenchSpec`. NOT
        /// zero-knowledge; receivers verify by recomputing.
        /// Real zk implementation lands in Wave 5.1+.
        #[derive(Debug, Clone, Copy, Default)]
        pub struct $name;

        impl BenchBackend for $name {
            fn name(&self) -> &'static str {
                $label
            }

            fn prove(&self, spec: &BenchSpec) -> Result<BenchProof, BenchError> {
                if spec.token_count == 0 {
                    return Err(BenchError::InvalidSpec("token_count must be > 0".into()));
                }
                let bytes = scaffold_commitment($label, spec).to_vec();
                Ok(BenchProof { backend: $label.to_string(), bytes })
            }

            fn verify(
                &self,
                spec: &BenchSpec,
                proof: &BenchProof,
            ) -> Result<(), BenchError> {
                if proof.backend != $label {
                    return Err(BenchError::VerifyFailed(format!(
                        "backend mismatch: expected {} got {}",
                        $label, proof.backend
                    )));
                }
                if proof.bytes.len() != 32 {
                    return Err(BenchError::VerifyFailed(
                        "proof bytes must be 32 bytes (SHA-256)".into(),
                    ));
                }
                let expected = scaffold_commitment($label, spec);
                if proof.bytes.as_slice() != expected.as_slice() {
                    return Err(BenchError::VerifyFailed("scaffold commitment mismatch".into()));
                }
                Ok(())
            }
        }
    };
}

#[cfg(feature = "ezkl")]
scaffold_backend!(EzklBackend, "ezkl");

#[cfg(feature = "risc0")]
scaffold_backend!(Risc0Backend, "risc0");

#[cfg(feature = "halo2")]
scaffold_backend!(Halo2Backend, "halo2");

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

    fn strength(&self) -> BackendStrength {
        BackendStrength::Cryptographic
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
// Phase 24 Wave 5.2 — risc0-zkvm host-side verifier
// ---------------------------------------------------------------------------
//
// What this backend does:
//
//   - `verify`: deserialise a real `risc0_zkvm::Receipt` from the
//     wire `bytes`, cryptographically verify the STARK against
//     the configured image ID, then cross-check the journal
//     commits to the exact `BenchSpec` we're verifying against.
//     A passing verify means the (model_hash, prompt_hash,
//     output_hash, token_count, flops) tuple was produced by a
//     trusted-image execution — `BackendStrength::InputOutputBound`.
//
// What this backend does NOT yet do (Wave 5.2.1+):
//
//   - `prove`: today returns `BackendUnavailable`. Real proving
//     requires a guest ELF + Risc-V toolchain prebuild. Wave 5.2
//     ships only the host-side wire-up so receivers can verify
//     proofs produced by other operators who have the toolchain.

#[cfg(feature = "risc0-host")]
pub mod risc0_host {
    use super::{BackendStrength, BenchBackend, BenchError, BenchProof, BenchSpec};
    use sha2::{Digest, Sha256};

    /// Canonical journal commitment a guest is expected to write —
    /// the host re-derives it from the `BenchSpec` and cross-checks
    /// against the journal in the receipt.
    pub fn expected_journal_commit(spec: &BenchSpec) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"tirami-risc0-commit-v1");
        h.update(spec.model_hash);
        h.update(spec.prompt_hash);
        h.update(spec.output_hash);
        h.update(spec.token_count.to_le_bytes());
        h.update(spec.flops.to_le_bytes());
        h.finalize().into()
    }

    /// Host-side risc0 backend. Holds the 32-byte image ID of the
    /// trusted guest ELF. Verifier-only until Wave 5.2.1+ adds the
    /// guest binary.
    #[derive(Debug, Clone, Copy)]
    pub struct Risc0HostBackend {
        pub image_id: [u8; 32],
    }

    impl Risc0HostBackend {
        pub const NAME: &'static str = "risc0-host";

        pub fn new(image_id: [u8; 32]) -> Self {
            Self { image_id }
        }
    }

    impl BenchBackend for Risc0HostBackend {
        fn name(&self) -> &'static str {
            Self::NAME
        }

        fn prove(&self, _spec: &BenchSpec) -> Result<BenchProof, BenchError> {
            // Wave 5.2.1+ wires the guest ELF + cargo-risczero
            // toolchain. Today the host crate is verifier-only.
            Err(BenchError::BackendUnavailable(
                "risc0-host: prove requires guest ELF (Wave 5.2.1+)",
            ))
        }

        fn verify(&self, spec: &BenchSpec, proof: &BenchProof) -> Result<(), BenchError> {
            if proof.backend != Self::NAME {
                return Err(BenchError::VerifyFailed(format!(
                    "wrong backend: {}",
                    proof.backend
                )));
            }
            // Step 1: bincode-decode the receipt.
            let receipt: risc0_zkvm::Receipt =
                bincode::deserialize(&proof.bytes).map_err(|e| {
                    BenchError::VerifyFailed(format!("receipt decode: {e}"))
                })?;
            // Step 2: cryptographically verify the STARK against
            // the trusted image ID. The risc0 crate handles the
            // FRI-based proof system internally.
            receipt
                .verify(self.image_id)
                .map_err(|e| BenchError::VerifyFailed(format!("receipt verify: {e}")))?;
            // Step 3: cross-check the public journal commits to
            // this exact BenchSpec. The receipt's STARK only
            // proves "the guest ran and committed *something*";
            // we still need to bind that something to the spec.
            let journal_commit: [u8; 32] = receipt.journal.decode().map_err(|e| {
                BenchError::VerifyFailed(format!("journal decode: {e}"))
            })?;
            let expected = expected_journal_commit(spec);
            if journal_commit != expected {
                return Err(BenchError::VerifyFailed(
                    "journal commit does not match spec".into(),
                ));
            }
            Ok(())
        }

        fn strength(&self) -> BackendStrength {
            BackendStrength::InputOutputBound
        }
    }
}

#[cfg(feature = "risc0-host")]
pub use risc0_host::Risc0HostBackend;

// ---------------------------------------------------------------------------
// Phase 24 Wave 2 — TradeAttestation <-> BenchProof conversion
// ---------------------------------------------------------------------------

use tirami_ledger::ledger::TradeAttestation;

impl From<BenchProof> for TradeAttestation {
    fn from(p: BenchProof) -> Self {
        TradeAttestation::new(p.backend, p.bytes)
    }
}

impl From<&BenchProof> for TradeAttestation {
    fn from(p: &BenchProof) -> Self {
        TradeAttestation::new(p.backend.clone(), p.bytes.clone())
    }
}

impl From<TradeAttestation> for BenchProof {
    fn from(t: TradeAttestation) -> Self {
        BenchProof { backend: t.backend, bytes: t.bytes }
    }
}

impl From<&TradeAttestation> for BenchProof {
    fn from(t: &TradeAttestation) -> Self {
        BenchProof { backend: t.backend.clone(), bytes: t.bytes.clone() }
    }
}

/// Verify a `TradeAttestation` attached to a `SignedTradeRecord`.
/// Dispatches by `backend` name. For `ed-attest`, verifies the
/// Ed25519 signature and enforces signer == `expected_signer`
/// (typically the trade's `provider`).
pub fn verify_trade_attestation(
    spec: &BenchSpec,
    attestation: &TradeAttestation,
    expected_signer: &[u8; 32],
) -> Result<(), BenchError> {
    let proof: BenchProof = attestation.into();
    match attestation.backend.as_str() {
        EdAttestBackend::NAME => {
            verify_ed_attest_proof_by_signer(spec, &proof, expected_signer)
        }
        MockBackend::NAME => MockBackend.verify(spec, &proof),
        _ => Err(BenchError::VerifyFailed(format!(
            "unknown attestation backend: {}",
            attestation.backend
        ))),
    }
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
    /// Phase 24 Wave 5.2 — host-side risc0 verifier wired to the
    /// real `risc0-zkvm` crate (feature `risc0-host`). `prove`
    /// still returns `BackendUnavailable` (guest ELF is Wave
    /// 5.2.1+), but `verify` cryptographically checks STARK
    /// receipts produced elsewhere. Strength: `InputOutputBound`.
    Risc0Host,
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
            Self::Risc0Host => "risc0-host",
            Self::Halo2 => "halo2",
        }
    }

    /// Phase 24 Wave 5.1 — the *taxonomy strength* of this kind of
    /// backend. Distinct from `is_available` (operational status):
    /// `is_available` says "can I prove/verify with this backend
    /// right now"; `strength` says "if I could, how strong would
    /// the proof be". An agent comparing peers prefers higher
    /// strength even when both are currently available.
    ///
    /// `Ezkl` / `Risc0` / `Halo2` report `None` here because the
    /// scaffold implementations behind their feature flags are not
    /// zero-knowledge. Wave 5.1+ bumps them to `InputOutputBound`
    /// when the real crates land; Wave 5.3+ bumps them to
    /// `ComputeBound` when the circuit verifies inference.
    pub fn strength(self) -> BackendStrength {
        match self {
            Self::Mock => BackendStrength::None,
            Self::EdAttest => BackendStrength::Cryptographic,
            // Scaffold backends are NOT zk; real risc0/ezkl/halo2
            // lands in Wave 5.1+ and bumps the strength.
            Self::Ezkl | Self::Risc0 | Self::Halo2 => BackendStrength::None,
            // Phase 24 Wave 5.2 — host-side real risc0 verifier
            // (prove still pending guest ELF, but verify
            // cryptographically checks STARK receipts).
            Self::Risc0Host => BackendStrength::InputOutputBound,
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

    // Default feature build: ezkl/risc0/halo2 are unavailable stubs.
    #[cfg(not(any(feature = "ezkl", feature = "risc0", feature = "halo2")))]
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

    // -----------------------------------------------------------------
    // Phase 24 Wave 5.0 — scaffold-backend property tests (feature-gated)
    // -----------------------------------------------------------------

    #[cfg(feature = "risc0")]
    #[test]
    fn risc0_scaffold_round_trip_succeeds() {
        let b = Risc0Backend;
        let s = spec();
        let proof = b.prove(&s).expect("prove");
        assert_eq!(proof.backend, "risc0");
        assert_eq!(proof.bytes.len(), 32);
        b.verify(&s, &proof).expect("verify");
    }

    #[cfg(feature = "risc0")]
    #[test]
    fn risc0_scaffold_is_deterministic() {
        let b = Risc0Backend;
        let s = spec();
        let p1 = b.prove(&s).expect("prove");
        let p2 = b.prove(&s).expect("prove");
        assert_eq!(p1.bytes, p2.bytes);
    }

    #[cfg(feature = "risc0")]
    #[test]
    fn risc0_scaffold_rejects_tampered_spec() {
        let b = Risc0Backend;
        let s = spec();
        let proof = b.prove(&s).expect("prove");
        let mut tampered = s.clone();
        tampered.prompt_hash[0] ^= 0xFF;
        let err = b.verify(&tampered, &proof).unwrap_err();
        assert!(matches!(err, BenchError::VerifyFailed(_)));
    }

    #[cfg(feature = "risc0")]
    #[test]
    fn risc0_scaffold_rejects_wrong_backend_label_on_proof() {
        let b = Risc0Backend;
        let s = spec();
        let mut proof = b.prove(&s).expect("prove");
        proof.backend = "ezkl".to_string();
        let err = b.verify(&s, &proof).unwrap_err();
        assert!(matches!(err, BenchError::VerifyFailed(_)));
    }

    #[cfg(feature = "risc0")]
    #[test]
    fn risc0_scaffold_rejects_zero_token_count() {
        let mut s = spec();
        s.token_count = 0;
        let err = Risc0Backend.prove(&s).unwrap_err();
        assert!(matches!(err, BenchError::InvalidSpec(_)));
    }

    #[cfg(feature = "risc0")]
    #[test]
    fn risc0_scaffold_label_is_part_of_canonical_preimage() {
        // Each scaffold backend produces a label-keyed commitment, so
        // even identical specs yield different bytes across backends.
        // Without that, an attacker could replay an `ezkl` proof under
        // the `risc0` label and pass receiver-side dispatch.
        #[cfg(feature = "ezkl")]
        {
            let s = spec();
            let r0 = Risc0Backend.prove(&s).unwrap();
            let ez = EzklBackend.prove(&s).unwrap();
            assert_ne!(r0.bytes, ez.bytes);
        }
    }

    #[cfg(feature = "risc0")]
    #[test]
    fn risc0_scaffold_runs_through_run_bench_harness() {
        let r = run_bench(&Risc0Backend, &spec(), 5).expect("bench");
        assert_eq!(r.backend, "risc0");
        assert_eq!(r.proof_bytes, 32);
        assert_eq!(r.trials, 5);
    }

    #[cfg(feature = "risc0")]
    #[test]
    fn risc0_scaffold_runs_through_run_bench_trade_attestation() {
        // Wave-2 verifier helper dispatches by backend name — confirm
        // the scaffold backend is rejected (it's not on the dispatch
        // matrix; only `mock` and `ed-attest` are). This is the
        // intended behaviour: scaffold proofs are NOT meant to pass
        // through `verify_trade_attestation` until Wave 5.1+ makes
        // them real zk.
        let s = spec();
        let proof = Risc0Backend.prove(&s).expect("prove");
        let att = TradeAttestation::new(proof.backend.clone(), proof.bytes.clone());
        let err = verify_trade_attestation(&s, &att, &[0u8; 32]);
        assert!(err.is_err());
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

    // ---- Phase 24 Wave 5.1 — BackendStrength taxonomy ----

    #[test]
    fn backend_strength_orders_monotonically() {
        // None < Cryptographic < InputOutputBound < ComputeBound.
        // Agents rely on this ordering to pick the strongest peer.
        assert!(BackendStrength::None < BackendStrength::Cryptographic);
        assert!(BackendStrength::Cryptographic < BackendStrength::InputOutputBound);
        assert!(BackendStrength::InputOutputBound < BackendStrength::ComputeBound);
    }

    #[test]
    fn backend_strength_as_str_is_stable_kebab_case() {
        assert_eq!(BackendStrength::None.as_str(), "none");
        assert_eq!(BackendStrength::Cryptographic.as_str(), "cryptographic");
        assert_eq!(BackendStrength::InputOutputBound.as_str(), "input-output-bound");
        assert_eq!(BackendStrength::ComputeBound.as_str(), "compute-bound");
    }

    #[test]
    fn backend_strength_serializes_as_kebab_case_for_wire() {
        let s = serde_json::to_string(&BackendStrength::InputOutputBound).unwrap();
        assert_eq!(s, "\"input-output-bound\"");
        let parsed: BackendStrength = serde_json::from_str("\"cryptographic\"").unwrap();
        assert_eq!(parsed, BackendStrength::Cryptographic);
    }

    #[test]
    fn mock_backend_strength_is_none() {
        assert_eq!(MockBackend.strength(), BackendStrength::None);
    }

    #[test]
    fn ed_attest_backend_strength_is_cryptographic() {
        let b = EdAttestBackend::generate();
        assert_eq!(b.strength(), BackendStrength::Cryptographic);
    }

    #[test]
    fn backend_kind_strength_agrees_with_backend_instance() {
        // The enum's strength() and the trait's strength() must
        // never disagree — the manifest reads from the enum, the
        // pipeline calls into the trait.
        assert_eq!(BenchBackendKind::Mock.strength(), MockBackend.strength());
        let ed = EdAttestBackend::generate();
        assert_eq!(BenchBackendKind::EdAttest.strength(), ed.strength());
    }

    #[test]
    fn backend_kind_strength_matches_tirami_core_taxonomy() {
        // The `tirami_core::zkml_backend_strength_tag` function is
        // the wire-format equivalent of `BenchBackendKind::strength`.
        // They MUST agree for every named backend, otherwise the
        // discovery manifest disagrees with the local trait dispatch.
        for kind in [
            BenchBackendKind::Mock,
            BenchBackendKind::EdAttest,
            BenchBackendKind::Ezkl,
            BenchBackendKind::Risc0,
            BenchBackendKind::Risc0Host,
            BenchBackendKind::Halo2,
        ] {
            assert_eq!(
                kind.strength().as_str(),
                tirami_core::zkml_backend_strength_tag(kind.name()),
                "kind {:?} disagrees: trait says {} but core says {}",
                kind,
                kind.strength().as_str(),
                tirami_core::zkml_backend_strength_tag(kind.name()),
            );
        }
    }

    // ---- Phase 24 Wave 5.2 — Risc0Host backend (host-side verifier) ----

    #[test]
    fn risc0_host_kind_name_is_kebab_case() {
        assert_eq!(BenchBackendKind::Risc0Host.name(), "risc0-host");
    }

    #[test]
    fn risc0_host_kind_strength_is_input_output_bound() {
        assert_eq!(
            BenchBackendKind::Risc0Host.strength(),
            BackendStrength::InputOutputBound,
        );
    }

    #[test]
    fn risc0_host_serialises_as_kebab_case_for_config() {
        let k = BenchBackendKind::Risc0Host;
        let s = serde_json::to_string(&k).unwrap();
        assert_eq!(s, "\"risc0-host\"");
        let parsed: BenchBackendKind =
            serde_json::from_str("\"risc0-host\"").unwrap();
        assert_eq!(parsed, BenchBackendKind::Risc0Host);
    }

    #[test]
    fn tirami_core_taxonomy_recognises_risc0_host_as_input_output_bound() {
        assert_eq!(
            tirami_core::zkml_backend_strength_tag("risc0-host"),
            "input-output-bound",
        );
    }

    // Feature-gated tests that touch the actual risc0_zkvm crate.
    // These exercise the verifier's malformed-input handling without
    // needing a real Receipt (which requires the Risc-V toolchain
    // landing in Wave 5.2.1+).

    #[cfg(feature = "risc0-host")]
    #[test]
    fn risc0_host_backend_name_matches_kind() {
        let b = Risc0HostBackend::new([0u8; 32]);
        assert_eq!(b.name(), BenchBackendKind::Risc0Host.name());
    }

    #[cfg(feature = "risc0-host")]
    #[test]
    fn risc0_host_backend_strength_via_trait_matches_kind() {
        let b = Risc0HostBackend::new([0u8; 32]);
        assert_eq!(b.strength(), BenchBackendKind::Risc0Host.strength());
        assert_eq!(b.strength(), BackendStrength::InputOutputBound);
    }

    #[cfg(feature = "risc0-host")]
    #[test]
    fn risc0_host_prove_returns_backend_unavailable_until_guest_lands() {
        // Wave 5.2 is host-verifier-only; prove requires the guest
        // ELF + Risc-V toolchain (Wave 5.2.1+). Calling prove must
        // fail cleanly, not panic.
        let b = Risc0HostBackend::new([0u8; 32]);
        let err = b.prove(&spec()).unwrap_err();
        assert!(matches!(err, BenchError::BackendUnavailable(msg) if msg.contains("guest ELF")));
    }

    #[cfg(feature = "risc0-host")]
    #[test]
    fn risc0_host_verify_rejects_wrong_backend_label() {
        let b = Risc0HostBackend::new([0u8; 32]);
        let proof = BenchProof {
            backend: "mock-sha256".into(),
            bytes: vec![0u8; 100],
        };
        let err = b.verify(&spec(), &proof).unwrap_err();
        assert!(matches!(err, BenchError::VerifyFailed(msg) if msg.contains("wrong backend")));
    }

    #[cfg(feature = "risc0-host")]
    #[test]
    fn risc0_host_verify_rejects_malformed_receipt_bytes() {
        // Random bytes are not a valid bincode-encoded Receipt;
        // verify must reject cleanly without panicking through
        // risc0-zkvm's deserialiser.
        let b = Risc0HostBackend::new([0u8; 32]);
        let proof = BenchProof {
            backend: "risc0-host".into(),
            bytes: vec![0xAB; 50],
        };
        let err = b.verify(&spec(), &proof).unwrap_err();
        assert!(matches!(err, BenchError::VerifyFailed(msg) if msg.contains("decode")));
    }

    #[cfg(feature = "risc0-host")]
    #[test]
    fn risc0_host_expected_journal_commit_is_deterministic() {
        use crate::risc0_host::expected_journal_commit;
        let a = expected_journal_commit(&spec());
        let b = expected_journal_commit(&spec());
        assert_eq!(a, b);
    }

    #[cfg(feature = "risc0-host")]
    #[test]
    fn risc0_host_expected_journal_commit_changes_with_spec_fields() {
        use crate::risc0_host::expected_journal_commit;
        let base = expected_journal_commit(&spec());
        let mut s2 = spec();
        s2.token_count += 1;
        assert_ne!(base, expected_journal_commit(&s2));
        let mut s3 = spec();
        s3.prompt_hash[0] ^= 0xFF;
        assert_ne!(base, expected_journal_commit(&s3));
    }

    #[cfg(feature = "risc0-host")]
    #[test]
    fn risc0_host_image_id_distinguishes_backend_instances() {
        // Different image IDs are different "trust roots" — a
        // receipt valid under image A must not pass under image B.
        // We can't construct a real receipt here, but we can at
        // least verify the field plumbs through Clone/Copy + the
        // image_id is structurally visible.
        let a = Risc0HostBackend::new([0xAAu8; 32]);
        let b = Risc0HostBackend::new([0xBBu8; 32]);
        assert_ne!(a.image_id, b.image_id);
    }

    #[test]
    fn scaffold_backends_strength_is_none_until_wave_5_1_lands() {
        // Sentinel test: when Wave 5.1+ wires real risc0-zkvm,
        // this test will need updating to assert InputOutputBound
        // (and Wave 5.3+ may bump to ComputeBound). The test
        // breaking is the signal to update docs + manifest
        // promises in lockstep with the implementation.
        assert_eq!(BenchBackendKind::Risc0.strength(), BackendStrength::None);
        assert_eq!(BenchBackendKind::Ezkl.strength(), BackendStrength::None);
        assert_eq!(BenchBackendKind::Halo2.strength(), BackendStrength::None);
    }

    // ---- Phase 24 Wave 2 — TradeAttestation conversion + verifier ----

    #[test]
    fn bench_proof_to_trade_attestation_round_trip() {
        let backend = EdAttestBackend::generate();
        let proof = backend.prove(&spec()).expect("prove");
        let att: TradeAttestation = (&proof).into();
        assert_eq!(att.backend, proof.backend);
        assert_eq!(att.bytes, proof.bytes);
        let back: BenchProof = (&att).into();
        assert_eq!(back.backend, proof.backend);
        assert_eq!(back.bytes, proof.bytes);
    }

    #[test]
    fn trade_attestation_signer_extraction_matches_pubkey() {
        let backend = EdAttestBackend::generate();
        let proof = backend.prove(&spec()).expect("prove");
        let att: TradeAttestation = proof.into();
        let extracted = att.ed_attest_signer().expect("signer");
        assert_eq!(extracted, backend.public_key_bytes());
    }

    #[test]
    fn trade_attestation_signer_none_for_wrong_backend() {
        let att = TradeAttestation::new("mock-sha256".into(), vec![0u8; 96]);
        assert!(att.ed_attest_signer().is_none());
    }

    #[test]
    fn trade_attestation_signer_none_for_wrong_length() {
        let att = TradeAttestation::new("ed-attest".into(), vec![0u8; 95]);
        assert!(att.ed_attest_signer().is_none());
    }

    #[test]
    fn verify_trade_attestation_ed_attest_succeeds_for_correct_signer() {
        let backend = EdAttestBackend::generate();
        let s = spec();
        let proof = backend.prove(&s).expect("prove");
        let att: TradeAttestation = proof.into();
        verify_trade_attestation(&s, &att, &backend.public_key_bytes())
            .expect("must verify");
    }

    #[test]
    fn verify_trade_attestation_rejects_signer_mismatch() {
        let alice = EdAttestBackend::generate();
        let bob = EdAttestBackend::generate();
        let s = spec();
        let proof = alice.prove(&s).expect("prove");
        let att: TradeAttestation = proof.into();
        let err = verify_trade_attestation(&s, &att, &bob.public_key_bytes());
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))));
    }

    #[test]
    fn verify_trade_attestation_rejects_tampered_bytes() {
        let backend = EdAttestBackend::generate();
        let s = spec();
        let proof = backend.prove(&s).expect("prove");
        let mut att: TradeAttestation = proof.into();
        // Flip a byte in the signature segment.
        att.bytes[64] ^= 0xFF;
        let err = verify_trade_attestation(&s, &att, &backend.public_key_bytes());
        assert!(matches!(err, Err(BenchError::VerifyFailed(_))));
    }

    #[test]
    fn verify_trade_attestation_dispatches_mock() {
        let s = spec();
        let proof = MockBackend.prove(&s).expect("mock prove");
        let att: TradeAttestation = proof.into();
        // expected_signer is ignored for mock; pass an arbitrary key.
        verify_trade_attestation(&s, &att, &[0u8; 32])
            .expect("mock attestation must verify");
    }

    #[test]
    fn verify_trade_attestation_rejects_unknown_backend() {
        let s = spec();
        let att = TradeAttestation::new("zerokitten".into(), vec![0u8; 32]);
        let err = verify_trade_attestation(&s, &att, &[0u8; 32]);
        assert!(matches!(err, Err(BenchError::VerifyFailed(msg)) if msg.contains("unknown")));
    }

    #[test]
    fn signed_trade_record_attestation_field_defaults_to_none() {
        // Pre-Wave-2 snapshots (without the attestation field) must
        // still deserialize cleanly via `#[serde(default)]`.
        use tirami_core::NodeId;
        use tirami_ledger::ledger::TradeRecord;
        let trade = TradeRecord {
            provider: NodeId([0u8; 32]),
            consumer: NodeId([1u8; 32]),
            trm_amount: 100,
            tokens_processed: 10,
            timestamp: 1_700_000_000_000,
            model_id: "m".to_string(),
            flops_estimated: 0,
            nonce: [0u8; 16],
        };
        let legacy = serde_json::json!({
            "trade": trade,
            "provider_sig": vec![0u8; 64],
            "consumer_sig": vec![0u8; 64],
        });
        let signed: tirami_ledger::ledger::SignedTradeRecord =
            serde_json::from_value(legacy).expect("legacy snapshot must load");
        assert!(signed.attestation.is_none());
    }

    #[test]
    fn signed_trade_record_round_trips_with_attestation() {
        use tirami_core::NodeId;
        use tirami_ledger::ledger::{SignedTradeRecord, TradeAttestation, TradeRecord};
        let backend = EdAttestBackend::generate();
        let proof = backend.prove(&spec()).expect("prove");
        let att: TradeAttestation = proof.into();
        let signed = SignedTradeRecord {
            trade: TradeRecord {
                provider: NodeId([0u8; 32]),
                consumer: NodeId([1u8; 32]),
                trm_amount: 50,
                tokens_processed: 5,
                timestamp: 1_700_000_000_000,
                model_id: "wave2".to_string(),
                flops_estimated: 0,
                nonce: [0u8; 16],
            },
            provider_sig: vec![0u8; 64],
            consumer_sig: vec![0u8; 64],
            attestation: Some(att.clone()),
        };
        let json = serde_json::to_string(&signed).expect("ser");
        let de: SignedTradeRecord = serde_json::from_str(&json).expect("de");
        assert_eq!(de.attestation, Some(att));
    }
}
