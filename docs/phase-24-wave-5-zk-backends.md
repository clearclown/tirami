# Phase 24 Wave 5 — Real zk backends

> Phase 24 Waves 1–4.5 built the full attestation pipeline using
> Ed25519 attestations: producers sign, receivers verify, gossip
> carries proofs end-to-end, governance can ratchet the policy
> upward and the trade-accept gate honours it. The remaining gap
> is the **strength of the claim** — ed-attest only proves "I
> signed this" not "I actually computed Y on X correctly".
>
> Wave 5 closes that gap with real zk-SNARK / zk-STARK proofs.

Status: **Wave 5.0 shipped** (scaffold backends behind feature
flags). **Wave 5.1+ pending** (real risc0/ezkl crate integration
— week-scale).

## What ed-attest proves vs. what zk would prove

| | ed-attest (Waves 1–4.5) | zk (Wave 5.1+) |
|---|---|---|
| Unforgeable claim about identity | ✅ | ✅ |
| Unforgeable claim about *computation* | ❌ — provider could lie about output | ✅ — circuit constrains output to be correct |
| Hides model weights from verifier | N/A | ✅ |
| Hides prompt from verifier | N/A | ✅ (depending on scheme) |
| Proof size | 96 bytes | 200 B – few KB (Groth16) / 50–500 KB (STARK) |
| Prove time | sub-ms | seconds to minutes |
| Verify time | sub-ms | ms–10 ms (Groth16) / 10–100 ms (STARK) |

## Wave 5.0 (✅ shipped) — Scaffold backends

The `risc0` / `ezkl` / `halo2` Cargo features now wire up a
**deterministic SHA-256 commitment scaffold** rather than returning
`BackendUnavailable`. The commitment is keyed by:

```
sha256(b"tirami-{label}-scaffold-v0" || canonical_bench_spec)
```

This is **not** zero-knowledge — receivers verify by recomputing —
but it lets:

1. Downstream code (trade attestation wire format, gossip verifier
   dispatch, `BenchBackendKind` selector) exercise non-ed-attest
   paths.
2. Property tests confirm the wire format is label-keyed (an
   `ezkl` proof can't be replayed under the `risc0` label).
3. CI benchmark the harness against a non-mock backend without
   pulling in the multi-GB risc0-zkvm dep chain.

Scaffold proofs are **explicitly rejected** by
`verify_trade_attestation` — see the
`risc0_scaffold_runs_through_run_bench_trade_attestation` test.
This is intentional: scaffolds are dev-only.

## Wave 5.1 — risc0-zkvm integration

### Goal

Replace `Risc0Backend`'s scaffold with a real risc0 STARK proof.
Guest computes `BenchSpec` validation: model_hash + prompt_hash
+ output_hash were correctly committed for the claimed
token_count + flops.

### Dependencies to add

```toml
# crates/tirami-zkml-bench/Cargo.toml
[dependencies.risc0-zkvm]
version = "1.0"
optional = true
default-features = false
features = ["std"]

[features]
risc0 = ["dep:risc0-zkvm"]
```

Note: `risc0-zkvm` ≈ 100 MB of compile artifacts + Risc-V
toolchain prebuild. Add a feature-flagged CI job; do **not**
enable on default workspace builds.

### Guest program (skeleton)

```rust
// guests/bench_commit/src/main.rs
use risc0_zkvm::guest::env;
use sha2::{Digest, Sha256};

fn main() {
    // Read public inputs: model_hash, prompt_hash, output_hash,
    // token_count, flops.
    let model_hash: [u8; 32] = env::read();
    let prompt_hash: [u8; 32] = env::read();
    let output_hash: [u8; 32] = env::read();
    let token_count: u64 = env::read();
    let flops: u64 = env::read();

    // Commit to all inputs publicly so the verifier can recompute
    // the journal hash and bind the receipt to this exact spec.
    let mut h = Sha256::new();
    h.update(b"tirami-risc0-commit-v1");
    h.update(model_hash);
    h.update(prompt_hash);
    h.update(output_hash);
    h.update(token_count.to_le_bytes());
    h.update(flops.to_le_bytes());
    let commit: [u8; 32] = h.finalize().into();

    env::commit(&commit);
}
```

This is still a "commit-only" circuit — it doesn't *verify*
inference correctness. That requires a circuit that runs a
quantised model forward pass, which is Wave 5.3.

### Host-side wiring

```rust
// crates/tirami-zkml-bench/src/risc0_backend.rs
#[cfg(feature = "risc0")]
impl BenchBackend for Risc0Backend {
    fn prove(&self, spec: &BenchSpec) -> Result<BenchProof, BenchError> {
        let env = risc0_zkvm::ExecutorEnv::builder()
            .write(&spec.model_hash)?
            .write(&spec.prompt_hash)?
            .write(&spec.output_hash)?
            .write(&spec.token_count)?
            .write(&spec.flops)?
            .build()?;

        let prover = risc0_zkvm::default_prover();
        let receipt = prover.prove(env, BENCH_COMMIT_ELF)?;

        let bytes = bincode::serialize(&receipt)
            .map_err(|e| BenchError::ProveFailed(e.to_string()))?;

        Ok(BenchProof { backend: "risc0".to_string(), bytes })
    }

    fn verify(&self, spec: &BenchSpec, proof: &BenchProof) -> Result<(), BenchError> {
        let receipt: risc0_zkvm::Receipt = bincode::deserialize(&proof.bytes)?;
        receipt.verify(BENCH_COMMIT_ID)?;
        // Cross-check the journal commitment matches the spec.
        let journal_commit: [u8; 32] = receipt.journal.decode()?;
        let expected = expected_commitment(spec);
        if journal_commit != expected {
            return Err(BenchError::VerifyFailed("journal mismatch".into()));
        }
        Ok(())
    }
}
```

### Tests

- `risc0_real_prove_then_verify_round_trip` — guarded by
  `#[cfg(feature = "risc0")]` and slow (multi-second prove).
- `risc0_real_verify_rejects_tampered_spec` — receipt verifies
  cryptographically but journal commitment binds to spec.
- `risc0_real_verify_rejects_tampered_journal` — receipt
  itself fails if the journal is rewritten.

### Wire-up

When `Config.zkml_backend == "risc0"` AND `feature = "risc0"`,
`pipeline.rs::produce_ed_attest_attestation` (now misnamed; will
be renamed `produce_attestation` in Wave 5.1) dispatches to
`Risc0Backend` instead of `EdAttestBackend`.

`verify_trade_attestation` gets a third dispatch arm for
`"risc0"` proofs.

## Wave 5.2 — ezkl integration

Same shape as 5.1 but using `ezkl` (Halo2-based, SNARK). Trade-off:

- ezkl can take an ONNX model as input and synthesise a circuit
  per layer — naturally aligned with the "prove this specific
  model's output" goal.
- Compile-per-model is expensive (one-time per model variant);
  per-inference prove is fast(er than risc0).
- Requires SRS file management — operators ship a 100 MB SRS
  the first time they run ezkl-attested inference.

## Wave 5.3 — model-forward circuit (research scope)

True zkML proof of inference: the circuit runs the model
forward pass and commits to the output. This is the property
the protocol promises but no backend currently delivers.

Open questions:

- Quantisation: int8 models prove tractably; bf16/f16 don't.
- Memory: full forward-pass circuits for 7B-parameter models
  blow up SRS sizes. Layer-wise sub-circuits + recursion are
  one promising direction.
- Batching: per-inference proofs are bandwidth-prohibitive for
  high-throughput nodes. Folding schemes (Nova, ProtoStar) let
  one proof commit to many inferences.

This is genuinely cutting-edge work; the protocol provides the
substrate (attestation wire format, governance ratchet) and
benefits from any backend that ships.

## Wave 5.4 — backend strength taxonomy

Once multiple real backends exist, the protocol needs a
machine-readable strength taxonomy so agents can route on it.

Sketch:

```rust
pub enum BackendStrength {
    /// Recomputable by anyone (mock, scaffold). Dev only.
    None,
    /// Unforgeable but not zk (ed-attest). Bound to a key.
    Cryptographic,
    /// zk-SNARK / zk-STARK over input/output commitment but
    /// not the computation (risc0-commit-only).
    InputOutputBound,
    /// zk over the computation itself (ezkl + ONNX model
    /// circuit, or risc0 guest that runs the model).
    ComputeBound,
}

impl BenchBackend {
    fn strength(&self) -> BackendStrength;
}
```

Agents reading the discovery manifest can prefer
`ComputeBound > InputOutputBound > Cryptographic > None`.

## What Wave 5 explicitly does NOT do

- **Mandate any specific backend.** The constitution doesn't
  pick winners; governance does, via the
  `ProofPolicy` + `BenchBackendKind` selectors.
- **Optimise prove time.** Real zk is slow today. Wave 5 ships
  *correct* proofs first; performance is a tracking concern.
- **Reach Wave 5.3 in one PR.** Model-forward circuits are
  weeks-to-months of cryptography + engineering work. Waves
  5.1 and 5.2 ship "commit-only" proofs which already provide
  meaningful protocol-level value (binding the trade to a
  specific input/output without revealing the prompt).
