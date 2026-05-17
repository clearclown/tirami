# Phase 24 — zkML real backends

> Phase 20-23 closed every 🟡 in the Status Honesty section
> *except* "real zkML proof-of-inference." `MockBackend` is a
> SHA-256 hash that anyone can recompute; `EzklBackend` /
> `Risc0Backend` / `Halo2Backend` are stubs that return
> `BackendUnavailable`. Phase 24 walks this from "trivially
> forgeable" to "production-grade proof of inference" without a
> single multi-week commit.

Status: Wave 1 + Wave 2 (data-plane) shipped.

## The trajectory

Phase 24 is structured as four waves of progressively stronger
proof, each bounded enough to ship as a single PR:

| Wave | Backend | Strength | Scope |
|---|---|---|---|
| 1 ✅ | **Ed25519 attestation** | "I attest with my key" — unforgeable, NOT zk | bounded (1 PR) |
| 2 | Protocol integration | trades carry signed `BenchProof`; verifiers cross-check signer ↔ provider | bounded (1 PR) |
| 3 | `risc0` / `ezkl` backend (one of) | first real zk proof of computation; opt-in feature flag | week-scale |
| 4 | Governance ratchet activation | network steps `ProofPolicy` Optional → Recommended → Required | bounded (1 PR) |

Wave 1 is shipped here. Waves 2-4 follow once Wave 1 lands.

## Wave 1 — Ed25519 attestation backend ✅ shipped

### What it proves (and doesn't)

`EdAttestBackend` produces a 96-byte `BenchProof`:

  bytes[0..32]   = signer's Ed25519 public key
  bytes[32..96]  = signature over the canonical `BenchSpec`

The canonical message is:

  sha256("tirami-zkml-ed-attest-v1" || model_hash || prompt_hash ||
         output_hash || token_count_le || flops_le)

**It proves:** the holder of the listed pubkey attests that this
exact `BenchSpec` is the input/output of an inference they ran.
The proof is **unforgeable** without the corresponding Ed25519
private key — qualitatively stronger than `MockBackend`'s
recompute-anywhere hash.

**It does not prove:** that the inference computation was
actually performed correctly. The signer could lie. zk-SNARK /
zk-STARK backends (Wave 3) cover that gap.

### Why ship this primitive now

Wave 2's protocol integration (attach signed `BenchProof` to
`SignedTradeRecord`) needs a backend it can *actually* call.
`MockBackend` is too weak — the trade would carry a public hash
anyone could forge. `EzklBackend` etc. aren't usable yet.
`EdAttestBackend` fills the bounded primitive gap so Wave 2 can
land without waiting on Wave 3's multi-week zk dep chains.

### Public surface

```rust
// crates/tirami-zkml-bench/src/lib.rs

pub struct EdAttestBackend { /* … */ }

impl EdAttestBackend {
    pub const NAME: &'static str = "ed-attest";
    pub fn from_signing_key(signing_key: SigningKey) -> Self;
    pub fn generate() -> Self;
    pub fn public_key_bytes(&self) -> [u8; 32];
}

impl BenchBackend for EdAttestBackend {
    fn name(&self) -> &'static str;
    fn prove(&self, spec: &BenchSpec) -> Result<BenchProof, BenchError>;
    fn verify(&self, spec: &BenchSpec, proof: &BenchProof) -> Result<(), BenchError>;
}

pub fn verify_ed_attest_proof(spec: &BenchSpec, proof: &BenchProof)
    -> Result<(), BenchError>;

pub fn verify_ed_attest_proof_by_signer(
    spec: &BenchSpec, proof: &BenchProof, expected_signer: &[u8; 32],
) -> Result<(), BenchError>;

pub enum BenchBackendKind {
    Mock, EdAttest, Ezkl, Risc0, Halo2,
}
```

The free-function verifiers (`verify_ed_attest_proof`,
`verify_ed_attest_proof_by_signer`) let the protocol layer
(`tirami-ledger` / `tirami-node`) check proofs without holding a
backend instance. Wave 2 wires them into the trade-receive path.

### Wave-1 integration with AgentIdentity

The `EdAttestBackend::from_signing_key` constructor is designed
so that a Phase-20-Wave-4 `AgentIdentity` can directly back an
attestation. Same key, same DID; the trade's `provider` field
and the proof's signer pubkey agree by construction. Wave 2
makes this binding mandatory: a `BenchProof` whose signer
doesn't match the trade's `provider` is rejected.

### Tests

17 new unit tests, all green:
- `ed_attest_prove_then_verify_round_trip`
- `ed_attest_verify_rejects_tampered_proof_bytes`
- `ed_attest_verify_rejects_tampered_spec`
- `ed_attest_verify_rejects_wrong_pubkey_in_proof`
- `ed_attest_verify_rejects_wrong_backend_name`
- `ed_attest_verify_by_signer_enforces_expected_pubkey`
- `ed_attest_two_backends_produce_different_proofs`
- `ed_attest_invalid_pubkey_bytes_reject_cleanly`
- `ed_attest_short_proof_bytes_rejected`
- `ed_attest_from_signing_key_round_trip`
- `ed_attest_canonical_includes_all_spec_fields` — verifies all 5
  spec fields contribute to the signed pre-image
- `ed_attest_token_count_zero_rejected`
- `ed_attest_runs_through_run_bench_harness` — backend slots into
  the existing `run_bench` harness for timing comparisons
- `backend_kind_default_is_mock`
- `backend_kind_availability_matrix`
- `backend_kind_names_match_backend_name_method`
- `backend_kind_serializes_as_kebab_case`

Workspace: **1,380 passed, 0 failed** (was 1,363 → +17 new).

## Wave 2 — data-plane integration ✅ shipped

This wave makes the protocol *able to carry* attestations end-to-end
without yet wiring the producer/verifier into `handle_inference`
(that is Wave 2.5, bounded follow-up). Specifically:

- ✅ `SignedTradeRecord` extended with optional
  `attestation: Option<TradeAttestation>` (`#[serde(default)]` —
  legacy pre-Wave-2 snapshots load unchanged).
- ✅ New on-trade `TradeAttestation { backend: String, bytes:
  Vec<u8> }` defined in `tirami-ledger` (workspace dependency
  direction kept acyclic — zkml-bench depends on ledger, not the
  reverse).
- ✅ `tirami-zkml-bench` ships `From<&BenchProof> for
  TradeAttestation` (and the inverse) plus
  `verify_trade_attestation(spec, &TradeAttestation,
  expected_signer)` which dispatches by `backend` name. For
  `ed-attest` it delegates to `verify_ed_attest_proof_by_signer`.
- ✅ `Config.zkml_backend: String` (default `"mock"`). Stored as
  a kebab-case string so `tirami-core` stays free of a
  `tirami-zkml-bench` dependency.
- ✅ Manifest exposes `zkml_backend` (top-level field on
  `/v1/tirami/protocol`) and the feature vector adds
  `zkml-backend:<name>` so agents can route on it.
- ✅ Backend-aware
  `tirami_core::advertised_protocol_features_with_backend(...)`.
  Old `advertised_protocol_features(...)` is preserved as a thin
  shim that defaults to `"mock"`.
- Tests: `tirami-zkml-bench` +9, `tirami-core` +6. Workspace
  passes 1,397 tests.

### Wave 2.5 (next, bounded) — pipeline produce + verify

The protocol now *carries* attestations but `pipeline.rs::handle_inference`
does not yet *produce* them, and `TradeGossip` does not yet *verify*
them. Wave 2.5 wires:

- Provider side (`handle_inference`): when `config.zkml_backend ==
  "ed-attest"` AND an `AgentIdentity` is loaded, construct a
  `BenchSpec` over (model_id_hash, prompt_hash, output_hash,
  total_tokens, flops_estimated), call
  `EdAttestBackend::from_signing_key(agent_identity.signing_key())
  .prove(spec)`, and attach the resulting `TradeAttestation` to
  the `SignedTradeRecord` before `execute_signed_trade`.
- Consumer/Gossip side: when receiving a `SignedTradeRecord` with
  `attestation: Some(_)`, call `verify_trade_attestation(spec,
  attestation, &trade.provider.0)`. Failure → reject the trade
  (in `proof_policy = "required"` mode) or downgrade reputation
  (in `recommended` mode).

Wave 2.5 is bounded but touches the inference path so it ships as
its own PR.

## Wave 3 — risc0 or ezkl integration

Pick one (likely `risc0` for cleaner Rust DX) and wire the real
proof-of-inference. Each is week-scale because:

- `risc0` requires a Risc-V toolchain prebuild and proof
  generation against an embedded ELF.
- `ezkl` requires SRS file management and circuit compilation
  per model.

Either backend produces a *zk* proof — the verifier learns the
output is correct *without* learning the model weights or
intermediate activations.

## Wave 4 — governance ratchet activation

`PROOF_POLICY_RATCHET` is already in
`IMMUTABLE_CONSTITUTIONAL_PARAMETERS`. Wave 4 introduces the
governance proposal flow that bumps the network-wide default
from `Optional` → `Recommended` → `Required`, with a holdback
period long enough that operators can configure their nodes
with the chosen backend before enforcement kicks in.

## What Phase 24 explicitly does NOT do

- **Replace MockBackend.** Even after all four waves,
  `MockBackend` stays available as a dev-only shape-testing
  primitive. The protocol layer rejects it for any policy above
  `Optional`.
- **Mandate zkML in Phase 24.** The ratchet is monotonic
  *forward*, but the *current epoch* of the constraint is
  governed by stakers, not by the code. The protocol just makes
  the enforcement possible.
