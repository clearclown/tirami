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
}
