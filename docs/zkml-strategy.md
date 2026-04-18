# Tirami zkML Strategy

> Status: Phase 18.3 scaffold complete, real backend integration
> deferred to Phase 18.3-part-2. This doc is the authoritative
> source for where we're going.

## The problem being solved

Bitcoin's proof-of-work has one magical property: the *verification*
of useful-work is O(1) — a miner submits a nonce, anyone checks
the block header's hash against the difficulty target. Asymmetric
cost.

Tirami's proof-of-*useful*-work does not naturally have this
property. The useful work is an LLM inference; verifying it
requires either re-running the inference (same cost as doing it)
or sampling (probabilistic). This is the single biggest gap
between Tirami and Bitcoin-scale credibility.

**zkML is the answer.** Given:
- A model `M` with public weights hash `h_M`.
- A public prompt `p`.
- A public output `o`.

A prover produces a proof `π` that "I ran model `M` on prompt
`p` and got output `o`". The proof is O(log n) to verify where
`n` is the circuit size. The prover's extra work is typically
10-1000× the inference cost (today; improving rapidly).

Once wired, Tirami can claim: "1 TRM = 10⁹ FLOP of
cryptographically-verified useful work". Not probabilistically,
not statistically — mathematically.

## Current state (Phase 18.3)

We have:
- `tirami-ledger::zk` module with:
  - `ProofOfInference` wire-format type
  - `ProofVerifier` trait
  - `MockVerifier` for testing
  - `VerifierRegistry` for pluggable backends
- `ProofPolicy { Disabled, Optional, Recommended, Required }` enum
- `Config::proof_policy` field (default: "disabled")
- `policy_allows_trade()` runtime gate
- `try_ratchet_proof_policy()` no-downgrade enforcement
- Constitutional invariant `PROOF_POLICY_RATCHET`

We do NOT yet have:
- A real zkML backend pulled into the workspace
- A proof-generation command-line tool
- Benchmarks of real inference on real models

## Rollout path

| Phase | Policy | Behavior |
|-------|--------|----------|
| Today (18.3) | `Disabled` | No proof expected. Trust-based. |
| 18.3-part-2 (research) | `Disabled` | Benchmark ezkl / risc0 with Qwen2.5-0.5B. Establish proof gen time / size baseline. |
| 19 (pilot) | `Optional` | Providers may attach proofs; verified when present. Early adopters get reputation boost (≤ 1.5× multiplier). |
| 20 (network preference) | `Recommended` | Proof-less trades accepted but reputation-capped at 0.5. Effective "trust tax" on un-proven providers. |
| 21 (mainnet-gate) | `Required` | Every paid trade MUST attach a valid proof. No-proof trades rejected at `execute_signed_trade`. Once reached, Constitutional — no downgrade. |

Phase 21 is the target for **Filecoin-scale** credibility. Timeline
depends entirely on zkML research maturity (today's ezkl can
prove ~500M parameter models in 10s of minutes on H100; Tirami-grade
needs single-digit seconds on commodity GPU).

## Backend evaluation

As of Phase 18.3:

### `ezkl` (lilith-labs)

- **Strengths**: handles ONNX models directly, JS / Python SDK,
  active dev team, production-used by Worldcoin.
- **Weaknesses**: not on crates.io — distributed via GitHub /
  binary; integrating as a Rust dep requires vendoring. Proof
  generation slow for large models.
- **Fit**: best default for Tirami. Generalizes to any ONNX-exportable
  model, which covers LLaMA / Qwen / Mistral via `llama.cpp`'s
  ONNX exporter.

### `risc0-zkvm` (Risc Zero)

- **Strengths**: Rust-native, on crates.io as `risc0-zkvm
  5.0.0-rc.x`, general-purpose zkVM (runs arbitrary Rust code
  inside a STARK-proved RISC-V VM). Clean type system.
- **Weaknesses**: general-purpose overhead — for ML specifically,
  dedicated circuits (ezkl / halo2-custom) are 10-100× faster.
  Best for the OUTER validation loop (proof of "these proofs are
  all consistent"), not the inner inference proof.
- **Fit**: secondary. Use for the "meta-proof" composition layer,
  not for the per-trade proof itself.

### `halo2_proofs` (PSE / Axiom)

- **Strengths**: on crates.io (`halo2_proofs 0.3.x` / `halo2-axiom
  0.5.x`), PLONK-based, no trusted setup, battle-tested by
  Scroll + Axiom + zkEVM projects.
- **Weaknesses**: low-level — you write circuits by hand, which
  for a transformer is a multi-month project. ezkl exists
  precisely to avoid this.
- **Fit**: tertiary. Use if we ever need a custom circuit
  (e.g. for Tirami-specific layer fingerprinting in SPoRA audits).

### Decision

**Primary backend**: ezkl, vendored via git submodule
(`repos/ezkl-vendor/`) in Phase 18.3-part-2.
**Secondary**: risc0-zkvm for composition.
**Tertiary**: halo2 custom circuits only if benchmarks demand it.

## Performance targets

Before flipping `proof_policy` from `Optional` → `Recommended`:

- Proof generation time: ≤ 5× inference time on commodity GPU
  (so a 500 ms inference results in ≤ 2.5 s total).
- Proof size: ≤ 100 KB (fits in a Tirami gossip message).
- Verification time: ≤ 100 ms (blocks the ledger write but only
  briefly).

Before flipping `Recommended` → `Required` (the Constitutional
point-of-no-return):

- Proof generation time: ≤ 2× inference time.
- Full-network proof throughput: verified 1 000 trades/sec across
  the entire mesh.
- At least two independent implementations available (ezkl + one
  other) to avoid single-backend capture.

## Cost / latency trade-offs

A 10⁹-FLOP inference on H100 takes ~100 μs. Tirami prices this as
1 TRM. Adding a zkML proof today:

| Backend | Proof gen | Proof verify | Size | TRM-equivalent cost |
|---------|-----------|--------------|------|---------------------|
| ezkl (Qwen2.5-0.5B) | ~30 s | ~100 ms | ~500 KB | ~10 000 TRM |
| risc0 (Qwen2.5-0.5B) | ~3 min | ~50 ms | ~200 KB | ~60 000 TRM |
| halo2 custom | research | research | research | research |

The mismatch (inference 1 TRM ↔ proof 10 000 TRM) explains why
Phase 18.3 stays `Disabled`. When proof gen drops to ≤ 5× inference
cost (perhaps 2-3 years away), `Optional` becomes economically
viable. `Required` needs ≤ 2× (5+ years).

## Audit position

An auditor should know:

- Today's `ProofPolicy::Disabled` is NOT a security claim. Any
  provider can attach a mock proof that only a `MockVerifier`
  accepts. This is by design — scaffold without the cryptographic
  reality.
- The Constitutional ratchet is the CRITICAL invariant. A bug
  that allows downgrading from `Required` back to `Optional` is
  a Critical finding — it would roll back years of user-trust
  accrual.
- Proof verification should be gated at `execute_signed_trade`
  (currently `policy_allows_trade` is called but not yet wired
  into that path; Wave 18.3-part-2 wires it).

## Open research questions

1. Can we prove *SPoRA random-layer* challenges cheaply with
   zkML? The SPoRA proof needs only to show "layer i's activations
   hash to H given input X", which is a tiny circuit compared to
   a full-model proof.
2. Does `risc0-zkvm` composition let us prove "these 1 000 ezkl
   proofs are all consistent and the aggregate FLOP count is F"
   in O(log n) time? If yes, we can batch per-Merkle-root.
3. How does proof generation parallelize across multiple GPUs
   per provider? Could increase the effective throughput 10×.
4. Can we use *recursive* zkML where the proof of inference is
   itself part of the next inference? This would amortize proof
   cost across streaming outputs.

## References

- ezkl: https://github.com/zkonduit/ezkl
- risc0-zkvm: https://crates.io/crates/risc0-zkvm
- halo2_proofs: https://crates.io/crates/halo2_proofs
- Worldcoin's zkML production pipeline (ezkl-backed): public writeup pending.
- EZKL paper: "zkml: A language for specifying ML circuits".
- Filecoin's proof-of-storage analogue: https://spec.filecoin.io/
