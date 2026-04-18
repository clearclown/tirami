# Tirami — Kani Formal Verification Proofs

**Wave:** Phase 17 Wave 3.2 · 2026-04-18.
**Status:** Initial 10-invariant set in place. External-audit
bar (Wave 3.3) is ≥ 30 invariants; follow-up waves will expand
coverage.

[Kani](https://model-checking.github.io/kani/) is a bit-precise
symbolic model checker for Rust. It compiles `#[kani::proof]`
harnesses, feeds symbolic inputs via `kani::any()` / `kani::assume`,
and attempts to prove each assertion holds for every input that
satisfies the assumptions. Where `cargo test` samples, Kani covers
exhaustively within a bounded state space.

Tirami's proofs live in `crates/tirami-ledger/src/kani_proofs.rs`,
gated behind `#[cfg(kani)]` so `cargo test` / `cargo build` ignore
them.

## Running

Install Kani once:

```bash
cargo install --locked kani-verifier
cargo kani setup
```

Then from the repo root:

```bash
# Run every proof in the ledger crate (primary target for Wave 3.2)
cargo kani --package tirami-ledger

# Run a single proof by name
cargo kani --package tirami-ledger --harness proof_apply_slash_burns_never_mints
```

Each proof emits a VERIFICATION:- SUCCESSFUL line on pass, with a
summary at the end. Failures include a concrete counterexample
trace, which Kani renders as a Rust-like execution path.

## Current invariant set

| # | Proof | What it asserts |
|---|-------|-----------------|
| 1 | `proof_nonce_cache_rejects_replay` | `NonceCache::insert` returns `false` on the second call for the same non-zero nonce. |
| 2 | `proof_nonce_cache_accepts_distinct` | Two distinct non-zero nonces are both accepted. |
| 3 | `proof_nonce_cache_bounded_by_capacity` | `order.len() <= CAPACITY` after any number of inserts. |
| 4 | `proof_v1_canonical_has_no_version_prefix` | v1 (zero-nonce) `canonical_bytes` length matches the legacy layout (88 + model_id.len()). |
| 5 | `proof_v2_canonical_has_version_prefix_and_nonce` | v2 bytes start with `CANONICAL_V2` and end with the 16-byte nonce. |
| 6 | `proof_v1_v2_canonical_bytes_never_collide` | For any provider/consumer/amt/ts/model, v1 and v2 bytes differ (different lengths). |
| 7 | `proof_apply_slash_on_unknown_node_returns_zero` | `StakingPool::apply_slash` on an unstaked node returns 0 and leaves `total_staked` at 0. |
| 8 | `proof_apply_slash_never_increases_total_staked` | For any pre-staked pool, `apply_slash` never mints (total_staked is monotone-non-increasing). |
| 9 | `proof_welcome_loan_limiter_honors_cap` | After `cap` grants in a bucket, `can_issue` returns false for the `cap+1`-th request. |
| 10 | `proof_nonce_cache_insert_is_idempotent_on_same_nonce` | Inserting an already-seen nonce does NOT grow the queue. |

## Expanding the set before external audit

Target additions for Wave 3.2-part-2:

- **TRM conservation across trade execution**: `execute_signed_trade`
  preserves `sum(balances.contributed - balances.consumed)` modulo
  newly-minted `total_minted`. Needs bounded `HashMap<NodeId, u64>`
  modelling — look at Kani's `HashMap` adapter patterns.
- **Signature-required balance growth**: a `NodeBalance.contributed`
  can never increase unless a signed record verifies. Requires
  threading the signature-verifier through the proof harness.
- **`apply_slash` penalty scale**: burned amount ==
  `compute_slash(stake, penalty)`, which for penalty = 0.3 (major)
  burns exactly 20 % of the stake. Property: `burned <= stake`.
- **`update_trust_penalties` cooldown**: a node slashed at time T
  cannot be slashed again before T + 300 000 ms.
- **`FraudProof::verify` rejects v1 records**: structural check.
- **`ValidatorQuorum::tally` returns Dissenter only when strict
  majority exists**: follows from the bucket-size comparison.
- **`AuditChallengeMsg::is_layer_scoped` correctness**: returns
  `true` iff `layer_index` is `Some(v)` and `v != FINAL_OUTPUT_LAYER`.
- **`HybridSignature::verify` fails if either half is wrong**:
  symbolic bit-flip over each half, both must trigger rejection.
- **`PeerRegistry::ensure` past capacity evicts the oldest**:
  invariant over the deque order.
- etc.

## Why Kani and not Prusti / Creusot

Kani is bit-precise (it uses CBMC underneath), free to use, works
on stable Rust, and has strong support from the Rust community
(Meta maintains it). Prusti / Creusot lean on verification logics
that we'd need to annotate extensively; Kani mostly lets us
assert Rust-level facts and get counterexamples. For the invariants
Tirami cares about (economic monotonicity, replay protection,
cap enforcement) Kani's bit-level reasoning is sufficient.

## Integrating into CI

Kani runs can be slow (minutes for non-trivial invariants). The
plan for CI:

- A separate GitHub Action workflow `kani.yml`, triggered on PRs
  touching `crates/tirami-ledger/src/**` or manually via
  `workflow_dispatch`.
- Job timeout 30 minutes.
- Failure blocks merge only on `main`; on feature branches, Kani
  failures emit a warning comment.

This CI wiring is Wave-3.2-part-2 (depends on getting Kani
reproducible in GitHub runners, which has had mixed results).

## Troubleshooting

**"error: failed to run custom build command for rdrand":**
Kani doesn't support every third-party crate. If Tirami adds a
dep that Kani can't handle, the affected proof needs to stub out
that code path or `kani::assume` a condition that bypasses it.

**"unwind value is too small":** increase the `#[kani::unwind(N)]`
attribute on the harness. For loop-heavy proofs, N = 4 or 8 is
often enough.

**Proof takes forever:** reduce the state space via more
`kani::assume` calls to narrow symbolic inputs to the relevant
range (e.g. `kani::assume(amt < 1000)`).
