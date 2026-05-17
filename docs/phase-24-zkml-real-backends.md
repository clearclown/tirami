# Phase 24 — zkML real backends

> Phase 20-23 closed every 🟡 in the Status Honesty section
> *except* "real zkML proof-of-inference." `MockBackend` is a
> SHA-256 hash that anyone can recompute; `EzklBackend` /
> `Risc0Backend` / `Halo2Backend` are stubs that return
> `BackendUnavailable`. Phase 24 walks this from "trivially
> forgeable" to "production-grade proof of inference" without a
> single multi-week commit.

Status: Waves 1 → 5.2 shipped. The host-side `risc0-zkvm` crate is
now wired (verifier-only); guest ELF + prover land in Wave 5.2.1+
(see `docs/phase-24-wave-5-zk-backends.md`).

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

### Wave 2.5 ✅ shipped — pipeline produce + signer enforcement

Wave 2.5 wires the producer path and the lightweight receiver-side
enforcement onto the data-plane Wave 2 added.

- ✅ `pipeline.rs::handle_inference` produces an ed-attest
  attestation when `config.zkml_backend == "ed-attest"` AND an
  `AgentIdentity` is loaded. Helpers:
  - `build_bench_spec(prompt, output_tokens, model_id,
    token_count, flops)` — pure function, deterministic SHA-256
    digest construction.
  - `produce_ed_attest_attestation(config, agent_identity, ...)`
    — returns `Some(TradeAttestation)` for the happy path,
    `None` when the backend is not `ed-attest`, no agent is
    loaded, or `token_count == 0`.
- ✅ `SignedTradeRecord::check_attestation_signer()` —
  lightweight check that the attestation's embedded signer
  pubkey (bytes[0..32]) equals `trade.provider`. Does NOT
  cryptographically verify the underlying Ed25519 signature
  yet — Wave 3 (full crypto) requires the wire format to carry
  prompt/output hashes so the receiver can rebuild the BenchSpec.
- ✅ `ComputeLedger::execute_signed_trade` enforces
  `check_attestation_signer` BEFORE recording the trade.
  Tampered or swapped attestations are rejected with
  `SignatureError::AttestationSignerMismatch` regardless of
  `proof_policy`.

#### Tests added

`tirami-ledger` +8: `check_attestation_signer_*` (5) + execute path
(3 — accepts valid, rejects tampered, no-attestation path
unchanged). `tirami-node::pipeline` +8: `build_bench_spec_*` (2) +
`produce_ed_attest_*` (6). Workspace passes **1,413** tests
(Wave 1 1,380 → Wave 2 1,397 → Wave 2.5 1,413).

### Wave 3 ✅ shipped — gossip wire + full crypto verify

Wave 3 widens the wire format and runs the full cryptographic
verifier at gossip-receive time.

- ✅ `tirami-proto::TradeGossip` gains two `#[serde(default)]`
  fields:
  - `attestation: Option<TradeAttestationWire>` — wire-mirror of
    `tirami_ledger::ledger::TradeAttestation`. Conversions live
    in tirami-ledger (proto < ledger, so the impls have to be
    on the ledger side).
  - `bench_spec_hint: Option<BenchSpecHint>` — the minimum
    information a receiver needs to rebuild the producer's
    `BenchSpec`: `model_hash`, `prompt_hash`, `output_hash`,
    and `flops`. `token_count` already rides on `tokens_processed`.
- ✅ `tirami_net::gossip::broadcast_trade(transport, gossip, signed, bench_spec_hint)` —
  signature widened (breaking-change for the single caller in
  `tirami-node::pipeline`, kept the no-cost convention for tests).
  Pipeline ships the hint when (and only when) `signed.attestation`
  is `Some`.
- ✅ `tirami_net::gossip::handle_trade_gossip` — when both
  `attestation` and `bench_spec_hint` are present, the receiver
  rebuilds `BenchSpec` from the hint and calls
  `tirami_zkml_bench::verify_trade_attestation`. Failure → drop
  the trade. Attestation present but hint missing → fall through
  to the dual-signature check only (degraded but not rejected).

#### Tests added

`tirami-net::gossip` +6:
- `handle_gossip_accepts_attested_trade_with_valid_hint`
- `handle_gossip_rejects_attestation_with_wrong_hint`
- `handle_gossip_skips_crypto_when_hint_absent`
- `handle_gossip_rejects_attestation_signed_by_non_provider`
- `handle_gossip_rejects_tampered_attestation_bytes`
- `trade_attestation_wire_round_trips_through_ledger_conversion`

Workspace passes **1,419** tests (Wave 2.5 1,413 → +6).

### Wave 4 ✅ shipped — governance ratchet activation

Wave 4 wires the proposal-driven upgrade path for `ProofPolicy`,
together with runtime state separate from the boot-time config
string.

#### What's new

- **`ProofPolicy::from_governance_value(v: f64) -> Option<Self>`** —
  parses the `new_value` of a `ChangeParameter` proposal. Rounds
  to nearest non-negative integer; rejects out-of-range and
  non-finite floats.
- **`GovernanceState::execute_proof_policy_proposal(id, current)`** —
  takes a Passed `ChangeParameter { name: "PROOF_POLICY", ... }`,
  applies `try_ratchet_proof_policy` (Constitutional no-downgrade),
  marks the proposal as `Executed`, and returns the new policy.
  Returns dedicated error variants for:
  - `ProposalNotPassed` — execute called on Active/Rejected/Executed
  - `UnsupportedExecution` — wrong parameter name / wrong proposal kind
  - `InvalidProofPolicyValue` — `new_value` outside 0..=3
  - `ProofPolicyDowngradeVetoed` — ratchet violated
- **`AppState.current_proof_policy: Arc<RwLock<ProofPolicy>>`** —
  the **runtime-enforced** policy, distinct from the boot-time
  string in `config.proof_policy`. Future code that gates
  trade-accept on policy reads this field.
- **`POST /v1/tirami/governance/execute/:id`** — applies a Passed
  PROOF_POLICY proposal. Status codes:
  - `200 OK` → `{ ok, proposal_id, previous_policy, new_policy, ratchet }`
  - `404` → proposal not found
  - `409 Conflict` → downgrade vetoed OR proposal not in Passed status
  - `400 Bad Request` → unsupported parameter / invalid value
- **`GET /v1/tirami/governance/proof-policy`** — read current
  policy: `{ policy, as_u8, ratchet }`.

#### What's NOT in Wave 4

The endpoints establish the **upgrade path**; they do not yet
**enforce** the new policy at trade-accept time. The runtime
`AppState.current_proof_policy` is the substrate; the gate at
`execute_signed_trade` still reads `proof_policy` from the boot
Config. Wiring the runtime substrate into the gate is a follow-up
Wave 4.5 (bounded — single function, plus tests).

#### Tests added

- `tirami-ledger::zk` +4: `from_governance_value` round-trip,
  rounding tolerance, out-of-range rejection, non-finite rejection
- `tirami-ledger::governance` +12: ratchet upgrade matrix,
  same-value idempotence, skip-steps allowed, downgrade vetoed,
  invalid value, NaN, not-passed, wrong-name, wrong-kind,
  unknown-id, execute-twice
- `tirami-node::handlers::governance` +6: HTTP-level happy/error
  paths for `/execute/:id` and `/proof-policy`

Workspace passes **1,441** tests (Wave 3 1,419 → +22).

### Wave 4.5 ✅ shipped — runtime gate hookup

The Wave-4 substrate now actually gates trade execution.

#### What's new

- **`ComputeLedger::execute_signed_trade_gated(signed, policy)`** —
  evaluates `policy_allows_trade(policy, has_attestation)` BEFORE
  the existing `execute_signed_trade` path. Failure returns
  `SignatureError::TradeRejectedByProofPolicy { policy, reason }`.
  The unparameterised `execute_signed_trade` is retained for tests
  / back-compat callers (behaviour ≡ `Disabled`).
- **`SignatureError::TradeRejectedByProofPolicy`** — new variant.
- **`TiramiNode.current_proof_policy: Arc<RwLock<ProofPolicy>>`** —
  shared with `AppState.current_proof_policy` (same Arc passed
  through `create_router_with_services`). A governance ratchet
  via `POST /v1/tirami/governance/execute/:id` is immediately
  visible to the pipeline.
- **`PipelineCoordinator::run_seed` takes the shared
  `current_proof_policy`** and threads it through
  `handle_inference` and the gossip-receive task. The producer
  path uses `current_proof_policy` as a snapshot (the gate value
  doesn't change mid-inference); the gossip-receive path reads
  it fresh per trade so late-arriving gossip respects the most
  recent governance state.

#### Reject path semantics

When `policy = Required` and an unattested trade arrives:

- **Local execution** (producer's own inference): the trade is
  rejected; the consumer reservation is already released
  upstream; reputation does *not* get the success boost.
- **Gossip receive**: rejected locally; the bilateral agreement
  between remote provider/consumer is unaffected — this only
  refuses to *record* the trade in this node's ledger.
- **Nonce slot is NOT consumed** on policy rejection — a
  follow-up attempt to record the same nonce with a valid
  attestation isn't auto-blocked as a replay.

#### Tests added

`tirami-ledger::execute_signed_trade_gated` matrix +9:
- `gated_disabled_accepts_unattested_trade`
- `gated_optional_accepts_unattested_trade`
- `gated_recommended_accepts_unattested_trade`
- `gated_required_rejects_unattested_trade`
- `gated_required_accepts_well_attested_trade`
- `gated_still_enforces_attestation_signer_match`
- `gated_still_enforces_nonce_dedup`
- `gated_required_with_optional_attestation_present_still_accepts`
- `gated_rejection_does_not_consume_nonce_slot`

Workspace passes **1,450** tests (Wave 4 1,441 → +9).

### Wave 5.0 ✅ shipped — scaffold backends behind feature flags

Pre-Wave-5.0 state: `Risc0Backend` / `EzklBackend` / `Halo2Backend`
were stubs returning `BackendUnavailable` even with their Cargo
feature enabled. Real risc0-zkvm / ezkl crate integration is
genuinely week-scale work (Risc-V toolchain, ~100 MB SRS files,
guest program builds), so it doesn't ship in one PR.

Wave 5.0 closes a narrower gap: when the `risc0` / `ezkl` /
`halo2` features are enabled, the backends produce a
**deterministic SHA-256 commitment scaffold** keyed by the
backend label and the full canonical `BenchSpec`. This is
explicitly **not zero-knowledge** — receivers verify by
recomputing — but it lets the downstream wiring be exercised:

- The `BenchBackendKind` selector dispatches to a non-mock backend.
- The wire format (`TradeAttestation { backend, bytes }`) carries
  a non-ed-attest proof through the gossip pipeline.
- An attacker can't replay an `ezkl` scaffold proof under the
  `risc0` label (the label is part of the canonical pre-image).
- The verifier dispatch in `verify_trade_attestation` correctly
  rejects scaffold proofs — they're dev-only and must not pass
  through the protocol's authoritative verifier.

#### Tests added

`tirami-zkml-bench` +8 (all `#[cfg(feature = "risc0")]`):
- `risc0_scaffold_round_trip_succeeds`
- `risc0_scaffold_is_deterministic`
- `risc0_scaffold_rejects_tampered_spec`
- `risc0_scaffold_rejects_wrong_backend_label_on_proof`
- `risc0_scaffold_rejects_zero_token_count`
- `risc0_scaffold_label_is_part_of_canonical_preimage`
- `risc0_scaffold_runs_through_run_bench_harness`
- `risc0_scaffold_runs_through_run_bench_trade_attestation` (asserts the scaffold proof is rejected by the protocol verifier — intended)

Default-feature workspace still passes **1,450** tests; with
`--features risc0` enabled, `tirami-zkml-bench` runs 44 tests
(was 37 → +7 active, plus the 8th replaces the no-longer-firing
`stub_backends_return_unavailable` which is now feature-gated
to the no-features build).

### Wave 5.1 ✅ shipped — backend strength taxonomy + manifest exposure

Wave 5.1 was originally scoped (in `docs/phase-24-wave-5-zk-backends.md`)
as "real risc0-zkvm integration" — but that is genuinely week-scale
(Risc-V toolchain, guest ELF, 100+ MB SRS files). Wave 5.4 in the
same doc was scoped as "backend strength taxonomy", which is the
prerequisite for real-zk landings to communicate their strength
machine-readably. Wave 5.1 was reslotted to ship the taxonomy
first; real risc0 / ezkl crate integration follows as Wave 5.2+.

#### What Wave 5.1 ships

- **`BackendStrength` enum** (in `tirami-zkml-bench`):
  - `None` — recomputable, no cryptographic claim (mock, scaffolds)
  - `Cryptographic` — unforgeable per private key, no compute proof (ed-attest)
  - `InputOutputBound` — zk over (input, output) commitment (Wave 5.2+ risc0/ezkl real)
  - `ComputeBound` — zk over the model forward pass itself (Wave 5.3+, research)
  - `PartialOrd` so agents can `peer.strength() > current_best.strength()` to route trades to the strongest peer.
- **`BenchBackend::strength()`** — default-method returns `None`;
  `MockBackend` keeps default, `EdAttestBackend` overrides to
  `Cryptographic`. Scaffold backends (`Risc0Backend` /
  `EzklBackend` / `Halo2Backend` under their feature flags)
  keep `None` — they're explicit dev-only.
- **`BenchBackendKind::strength()`** — same taxonomy at the
  enum level, used by the discovery manifest.
- **`tirami_core::zkml_backend_strength_tag(backend: &str)`** —
  wire-format equivalent. Lets `tirami-core` (which can't
  depend on `tirami-zkml-bench`) advertise the strength.
- **Manifest exposure**: `/v1/tirami/protocol` now includes
  `zkml_strength` ("none" / "cryptographic" / "input-output-bound"
  / "compute-bound") and the protocol feature vector adds
  `zkml-strength:<tag>`.
- **Cross-taxonomy invariant test**:
  `backend_kind_strength_matches_tirami_core_taxonomy` — for
  every named backend, the trait's `strength()` must agree with
  the wire-format function. Breaking this catches divergence
  between the local runtime and the discovery manifest.
- **Sentinel test**:
  `scaffold_backends_strength_is_none_until_wave_5_1_lands`
  asserts the scaffold strength is still `None`. Wave 5.2+
  bumping it to `InputOutputBound` *must* be a coordinated
  change to this test + docs + manifest — keeps drift loud.

#### Tests +14

`tirami-core` +7: strength_tag matrix (known backends + unknown
fallback + case normalisation) + advertised-feature inclusion
across backends.

`tirami-zkml-bench` +7: BackendStrength ordering + serde +
per-backend trait/kind agreement + cross-taxonomy invariant +
sentinel for scaffold strength.

Workspace passes **1,464** tests (Wave 5.0 1,450 → +14).

### Wave 5.2 ✅ shipped — host-side `risc0-zkvm` verifier

Wave 5.2 wires the real `risc0-zkvm` crate (v1.2.6) as a new
`risc0-host` opt-in feature. The host crate gives the protocol a
production-grade STARK verifier *today* without waiting for the
Risc-V toolchain bootstrap that proving requires — receivers
running this build can verify proofs produced by other operators
who *do* have the toolchain.

#### What's new

- **`Risc0HostBackend`** in `tirami-zkml-bench::risc0_host`
  (feature `risc0-host`). Carries a 32-byte image ID and:
  - `verify(spec, proof)`: bincode-decodes a `risc0_zkvm::Receipt`
    from `proof.bytes`, cryptographically verifies the STARK
    against the image ID, then cross-checks the public journal
    commits to the canonical `BenchSpec` hash (`tirami-risc0-commit-v1`
    domain-separated).
  - `prove(...)`: `BackendUnavailable("guest ELF required")` —
    Wave 5.2.1+ wires the guest binary.
  - `strength()` → `BackendStrength::InputOutputBound` (real zk
    verification of input/output commitment).
- **`BenchBackendKind::Risc0Host`** variant — distinct from the
  Wave 5.0 scaffold `Risc0` (which stays around for dev). Name:
  `"risc0-host"`. Serialises as `"risc0-host"` (kebab-case).
- **`tirami_core::zkml_backend_strength_tag("risc0-host")`** →
  `"input-output-bound"` — agents reading the manifest can route
  to a strong-verify peer.
- **Manifest feature flag**:
  `FEATURE_ZKML_BACKEND_RISC0_HOST = "zkml-backend:risc0-host"`.
- **`bincode = workspace`** dep added to tirami-zkml-bench
  (used only under the `risc0-host` feature for receipt codec).

#### Honest scope

Wave 5.2 is **verifier-only**. The wire format for `risc0-host`
proofs is now fixed — `bincode(Receipt)` bytes — and any node
running this build can cryptographically check those proofs. But
no node in this build can *produce* a `risc0-host` proof. That
requires the guest ELF, which requires `cargo-risczero` + Risc-V
toolchain (Wave 5.2.1).

A consequence: in a network where most nodes run this build,
`zkml_backend = "risc0-host"` in any node's Config is a
**verifier-only** declaration. The producer side falls back to
`MockBackend` for `prove` (or whatever Wave 5.2.1+ wires next).

#### Tests added

**Default features (4 new, no risc0 dep):**
- `risc0_host_kind_name_is_kebab_case` — `BenchBackendKind::Risc0Host.name() == "risc0-host"`
- `risc0_host_kind_strength_is_input_output_bound`
- `risc0_host_serialises_as_kebab_case_for_config`
- `tirami_core_taxonomy_recognises_risc0_host_as_input_output_bound`

**Feature-gated `risc0-host` (8 new, real risc0_zkvm crate):**
- `risc0_host_backend_name_matches_kind`
- `risc0_host_backend_strength_via_trait_matches_kind`
- `risc0_host_prove_returns_backend_unavailable_until_guest_lands`
- `risc0_host_verify_rejects_wrong_backend_label`
- `risc0_host_verify_rejects_malformed_receipt_bytes` — exercises
  the bincode + risc0_zkvm deserialiser robustness
- `risc0_host_expected_journal_commit_is_deterministic`
- `risc0_host_expected_journal_commit_changes_with_spec_fields`
- `risc0_host_image_id_distinguishes_backend_instances`

Plus the Wave 5.1 invariant `backend_kind_strength_matches_tirami_core_taxonomy`
now iterates `Risc0Host` and would catch any drift between the
enum's `strength()` and the wire-format tag.

Workspace passes **1,468** tests under default features (Wave
5.1 1,464 → +4). With `--features risc0-host`, tirami-zkml-bench
runs 57 tests (was 51 → +6 active feature-gated; the 8 listed
above include 2 that are also-counted as the default-feature
ones, hence +6 vs +8).

### Wave 5.2.1 (next, bounded but toolchain-dependent) — guest ELF

To enable real `prove()` on this build:

1. Add a new in-workspace crate `tirami-zkml-bench-guest`
   containing the `bench_commit` guest program (target
   `riscv32im-risc0-zkvm-elf`).
2. Build with `cargo-risczero build` to produce a deterministic
   ELF + image ID. Embed via `methods!` macro into the host
   crate.
3. `Risc0HostBackend::prove` calls `risc0_zkvm::default_prover()`
   with the ELF; produces a `Receipt` containing the journal
   commitment.
4. CI: run `--features risc0-host` build but skip the slow
   prove-side tests on the default workflow.

### Wave 5.3 (week-scale) — ezkl + halo2 mirroring 5.2

Same shape as 5.2 for ezkl/halo2: host-side verifier first, then
guest/circuit binding.

### Wave 5.4 (research scope) — model-forward circuits

Lifts the strength of the relevant backend(s) from
`InputOutputBound` to `ComputeBound`.

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
