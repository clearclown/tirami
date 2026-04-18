# Changelog

All notable changes to Tirami are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Version
numbers follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Phase 17 — Large-Scale Security Hardening (Wave 1 in progress, 2026-04-18)

Prepares the protocol for adversarial public deployment. Wave 1 closes
the P0 integrity gaps identified by the Phase 17 security audit
(see `docs/threat-model.md`). All changes are backward-compatible via
`#[serde(default)]` on wire/snapshot types and a legacy-bearer fallback
on the auth middleware.

**1.1 — TradeRecord v2 (nonce + canonical bytes)**
- `TradeRecord` gains a 128-bit `nonce`; zero-nonce trades keep the v1
  byte layout, non-zero nonces use a version-prefixed v2 layout so
  signatures never collide across versions.
- `TradeRecord::fresh_nonce()` helper (OsRng).

**1.2 — execute_signed_trade enforces replay protection**
- `ComputeLedger::seen_nonces` + bounded `NonceCache` (10 K/provider).
- `SignatureError::ReplayedNonce` on v2 nonce reuse; consumer is not
  double-debited. Rebuilt from `trade_log` on restart.
- `TradeProposal` / `TradeGossip` wire types carry the nonce.
- Gossip receive + post-accept paths route through
  `execute_signed_trade` (the main wire-level replay attack surface).

**1.3 — Slashing wired into production**
- New `SlashEvent` type + persisted `slash_events` audit trail.
- `ComputeLedger::update_trust_penalties(&mut StakingPool, now_ms)`
  runs the collusion detector and burns stake via `apply_slash` when
  the trust penalty ≥ 0.1. 5-minute per-node cooldown.
- `TiramiNode::spawn_slashing_loop` — runs every
  `config.slashing_interval_secs` (default 300 s, clamped ≥ 60 s).
- `GET /v1/tirami/slash-events` exposes the audit trail.

**1.4 — AuditVerdict → slashing bridge**
- `ComputeLedger::record_audit_failure_slash` ties
  `AuditVerdict::Failed` to a 30 % ("major") stake burn plus an
  `"audit-fail"` SlashEvent.
- Pipeline `Payload::AuditResponse` handler now calls it, so a failed
  audit both demotes the tier AND burns stake (previously only the
  former, per the Phase 14.3 scaffold).

**1.5 — Per-node scoped API tokens**
- New `crates/tirami-node/src/api_tokens.rs`: `ApiScope`
  (ReadOnly / Inference / Economy / Admin), `ApiToken`, `TokenStore`.
- Raw tokens are 32 random bytes (hex-encoded); only the SHA-256 hash
  is persisted.
- Admin endpoints:
  - `POST /v1/tirami/tokens/issue` (Admin) — mint a scoped token with
    a human-readable label and TTL. Raw token shown exactly once.
  - `POST /v1/tirami/tokens/revoke` (Admin) — idempotent revocation
    by hash-hex.
  - `GET /v1/tirami/tokens` (Admin) — list metadata for active tokens.
- Middleware: the legacy `config.api_bearer_token` still works (treated
  as implicit Admin); alternatively a scoped token from the store is
  accepted for non-admin endpoints. `require_admin_scope` helper gates
  privileged handlers.

**1.6 — Post-quantum hybrid signature scaffold**
- New `crates/tirami-core/src/crypto.rs`:
  - `HybridSignature { ed25519_sig, pq_sig: Option<Vec<u8>>, pq_vk }`
    — Serde-friendly; degrades to pure Ed25519 when `pq_sig` is `None`
    so pre-Phase-17 peers interop cleanly.
  - `HybridKey::sign(msg)` + `HybridSignature::verify(vk, msg, pq_verifier)`
    with both-or-fail semantics when the PQ half is present.
  - `PqSigner` / `PqVerifier` traits to keep the underlying scheme
    pluggable (ML-DSA, Falcon, future).
  - `MockPqSigner` / `MockPqVerifier` (deterministic SHA-256 based)
    for scaffold-era tests.
- New `Config::pq_signatures: bool` (defaults to `false`) — flips the
  daemon between pure Ed25519 and full hybrid signing once the real
  ML-DSA backend lands.
- Why scaffold-only: the `ml-dsa` 0.1.0-rc.8 crate pulls a
  conflicting `digest 0.11` against iroh 0.97's locked
  `digest 0.11.0-rc.10`. Full type lattice + 12 verify-matrix tests
  are already in place so the swap to a real PQ backend is one file.

**Wave 1 test coverage (workspace):** 891 → 940 passing (+49 new
across Waves 1.1 – 1.6). `bash scripts/verify-impl.sh` remains GREEN
(123 / 123).

### Phase 17 Wave 2 — P1 Scale Hardening (2026-04-18)

Addresses the scale-break findings: designs that work at 10 nodes but
fall over at 1 000+ due to unbounded memory, trivial Sybil vectors,
and non-existent fork recovery. Every wave-2 item ships as a tested
primitive with clear production wire-up points.

**2.6 — PeerRegistry LRU bound**
- `PeerRegistry` gains a `VecDeque<NodeId>` access queue + `capacity`
  field (default 10 000 via `DEFAULT_PEER_REGISTRY_CAPACITY`).
- All mutating paths go through `ensure()`, which touches LRU and
  evicts the oldest peer on insert past capacity.
- Read-only `get()` does NOT touch LRU so a poller can't pin
  every entry.
- `restore_access_order()` seeds the queue by `last_seen` on loads
  of pre-Wave-2.6 snapshots.

**2.3 — Per-ASN rate limiter**
- New `crates/tirami-net/src/asn_rate_limit.rs`: `AsnResolver` trait,
  `StaticAsnResolver` (in-memory / testing), `AsnRateLimiter` with
  per-ASN `TokenBucket` (default 5 000 msg/s sustained, 10 000 burst).
- 50 IPs inside one ASN share ONE bucket, collapsing the cloud-Sybil
  multiplier from N× to 1×.
- `Config::asn_rate_limit_enabled` (default off) opts in.

**2.1 — SPoRA-style random-layer audit**
- Wire: `AuditChallengeMsg.layer_index: Option<u32>`,
  `AuditResponseMsg` echoes it back, `FINAL_OUTPUT_LAYER = u32::MAX`
  sentinel. `#[serde(default)]` for pre-Phase-17 interop.
- `AuditTracker::{issue,resolve}_at_layer` with
  `None ↔ Some(FINAL_OUTPUT_LAYER)` normalization.
- `InferenceEngine::generate_audit_at_layer` trait method with a
  layer-index-mixed default; backend-specific overrides hashing real
  intermediate activations land in the follow-up.
- Layer mismatch → `AuditVerdict::Unknown` (never flip tier on
  ambiguous evidence).

**2.2 — Probabilistic heavy-audit scaffold**
- New `crates/tirami-ledger/src/audit_snark.rs`:
  `AuditSeverity { Light, Heavy }`, `HeavyAuditConfig`
  (default 1 % sample / 3 validators, odd required),
  `ProbabilisticSampler`, `ValidatorQuorum`, `QuorumVerdict`.
- `Dissenter` verdict identifies the slashable validator;
  `Inconclusive` (tie or under-minimum) explicitly does NOT slash.
- SNARK compression (ezkl / risc0) deferred to Phase 18; the
  tri-validator quorum delivers most of the security benefit today.

**2.4 — Trade-log snapshotting + JSON-lines archive**
- New `crates/tirami-ledger/src/checkpoint.rs`: Bitcoin-style Merkle
  root over `TradeRecord::canonical_bytes`, `ArchivePath` option
  wrapper, append-only JSON-lines I/O with `sync_data`.
- `ComputeLedger::seal_and_archive(cutoff, now, archive)` partitions
  `trade_log` at cutoff, appends sealed slice to the archive (rolled
  back on I/O error), and records a `LedgerCheckpoint`. Idempotent
  on empty ranges.
- `checkpoints` field persists via `PersistedLedger`; a restart
  preserves the full audit trail.

**2.5 — Fork detection + nonce-collision fraud proofs**
- New `crates/tirami-ledger/src/fork.rs`:
  `ForkDetector::verdict(local_root, min_obs)` → `Converged`,
  `InMinority`, `NoQuorum`. Strict majority; tie → NoQuorum.
- `NonceFraudProof` bundles two distinct `SignedTradeRecord` sharing
  `(provider, nonce)`; `verify()` confirms provider eq, nonce eq,
  records distinct, both non-v1, both signatures valid. Broadcast
  and slash the accused.
- `detect_nonce_conflict(batch)` surfaces the first conflict.

**2.8 — Welcome-loan Sybil strengthening**
- New `crates/tirami-ledger/src/sybil.rs`: `WelcomeLoanLimiter` with
  per-bucket rolling 24-h window (default 10 grants),
  10× `STAKED_THRESHOLD_MULTIPLIER` when the requester is
  stake-proven.
- Bucket key is caller-supplied `String` — operators can key by
  ASN, subnet prefix, GeoIP country, or any future dimension
  without recompilation.

**2.7 — Base Sepolia deployment scaffold + runbook**
- New `crates/tirami-anchor/src/base_client.rs`:
  `BaseChainMode { Sepolia, Mainnet }` with chain IDs and default
  RPC URLs, `BaseSepoliaConfig` (persistable, address-validated),
  scaffolded `BaseClient: ChainClient` that returns
  `ChainError::NotImplemented` on writes. Switch-over is one file
  once the `digest 0.11` dep pin resolves.
- `BaseSepoliaConfig::mainnet_reserved` is `#[deprecated]` pointing
  at the audit gate — any code unlocking mainnet before Wave 3.3
  triggers a loud lint.
- Full deployment runbook at
  `docs/phase-17-wave-2.7-base-deployment.md` covering Foundry
  install, dry-run, broadcast, Basescan verification, 30-day
  stability watch, and the mainnet gate.

**Wave 2 test coverage:** 940 → 1 040 passing (+100 across all six
waves). `cargo build --workspace` clean.

Mainnet deploy remains **BLOCKED** until:
1. External security audit complete (Wave 3.3).
2. 30-day Sepolia stability.
3. Multi-sig custody configured and tested on Sepolia.
4. Bug bounty live ≥ 30 days.

### Phase 17 Wave 3 — P2 Hostile-Environment Readiness (2026-04-18)

Closes the Phase 17 plan. Every item targets operating Tirami in a
truly public, adversarial setting — where operator key hygiene,
DDoS, formal-verification evidence, and professional disclosure
processes all matter.

**3.3 — External audit preparation docs**
- `docs/security/audit-scope.md` — in/out of scope, candidate
  auditor shortlist (Trail of Bits / OpenZeppelin / Zellic /
  Least Authority / Runtime Verification), feature-freeze rules,
  deliverables, mainnet-gate checklist.
- `docs/security/threat-model-v2.md` — 27 threats re-scored with
  residual risk + per-threat mitigation pointer + wave reference.
- `docs/security/known-issues.md` — every K-### issue identified
  but not fully remediated (9 open, 8 resolved, 3 "considered and
  not fixed").

**3.6 — SECURITY.md bug-bounty framework**
- Bug-bounty scale (Critical $25-50k, High $5-20k, Medium $1-4k,
  Low $200-800).
- Rules of engagement with good-faith legal safe harbor.
- PGP key placeholder (operator replaces before program goes live).
- Hall-of-Fame template.

**3.4 — DDoS mitigation**
- New `Config::max_concurrent_connections: u32` (default 1 000).
- `docs/operator-guide.md` gains a 7-section DDoS block:
  Cloudflare / Caddy / nginx frontends (with Caddyfile example),
  per-node connection cap, pointer to Wave 2.3 per-ASN limiter,
  OS-level tweaks, backpressure, Prometheus alerts, incident
  runbook.

**3.5 — Key rotation scaffold**
- New `crates/tirami-core/src/key_rotation.rs`:
  `NodeIdentity` with `Vec<KeyEpoch>`, `KeyState { Active, Revoked }`,
  `rotate(now_ms)` creates a new Active epoch and revokes the old
  with a timestamp.
- `verify_historical(identity, msg, sig, signing_at_ms)`: Active
  keys verify anytime; Revoked keys verify only when
  `signing_at_ms <= revoked_at_ms`. Rejects new signatures under
  a revoked key.
- 12 tests covering rotation, historical verification, serde
  roundtrip, error paths.

**3.2 — Kani formal-verification invariants**
- New `crates/tirami-ledger/src/kani_proofs.rs` (gated behind
  `#[cfg(kani)]`) with 10 initial invariants:
  nonce-cache replay rejection, canonical-bytes v1/v2 separation,
  `apply_slash` burn-only monotonicity, welcome-loan cap honouring,
  nonce-cache idempotency.
- `docs/security/kani-proofs.md` — how to install + run, invariant
  table, target additions for Wave-3.2-part-2 (≥ 30 before external
  audit), CI integration plan.

**3.1 — Hardware attestation scaffold**
- New `crates/tirami-core/src/attestation.rs` with
  `AttestationKind { Mock, AppleSecureEnclave, NvidiaH100CC,
  IntelSgx, AmdSevSnp }`, `AttestationReport { kind, node_id,
  evidence, issued_at, valid_until }`, `AttestationProvider` /
  `AttestationVerifier` traits.
- `MockAttestationProvider` + `MockAttestationVerifier`:
  SHA-256-based deterministic pair for tests.
- `ATTESTED_AUDIT_TIER_SPEED_MULTIPLIER = 5` constant.
- `AttestationPreferences` (opt-in routing preference).
- 14 tests covering provider↔verifier round-trip, peer mismatch,
  stale / pre-issuance rejection, kind mismatch, tampered
  evidence, wrong-length evidence, serde roundtrip.

Real Apple SE / NVIDIA H100 CC bindings are deferred —
`security-framework` and `nvml-wrapper` integrations land once we
have operator testbeds on that hardware.

**Wave 3 test coverage:** 1 040 → 1 066 passing (+26 new tests
across the six waves). `cargo build --workspace` clean. Kani
invariants invisible to the regular build (cfg-gated).

**Phase 17 total:** 891 → 1 066 passing (+175 new tests across
Waves 1+2+3, spanning 20 primitives). `verify-impl.sh` GREEN
(123 / 123) throughout.

### Phase 14 — Unified Scheduler (2026-04-14 → 2026-04-17)

Brings the v2 reference implementation's "Ledger-as-Brain" architecture
into the production v1 codebase. The pipeline and economic engine now
share state through the PeerRegistry + InferenceTicket pattern.

**14.1 — PeerRegistry + PriceSignal gossip**
- `tirami_core::PriceSignal`, `AuditTier` types.
- `tirami_ledger::peer_registry::{PeerRegistry, PeerState}`.
- New `Payload::PriceSignalGossip` wire variant.
- 30-second periodic gossip loop in `TiramiNode::run_seed`.
- New `GET /v1/tirami/peers` endpoint.

**14.2 — select_provider + InferenceTicket**
- `ComputeLedger::select_provider / begin_inference / settle_inference`.
- Atomic schedule + reserve + settle flow via `InferenceTicket`.
- New `TiramiError` variants: `SchedulingError`, `InsufficientBalance`.
- New `POST /v1/tirami/schedule` read-only probe.

**14.3 — Audit protocol skeleton**
- `Payload::AuditChallenge / AuditResponse` wire messages + validation.
- `peer_registry::record_audit_result` tier progression.
- Pipeline dispatch scaffolds (full loop deferred to Phase E).
- Issue #61 fix: `X-Tirami-Node-Id` attributes bilateral trades.

### Phase 15 — Product redefinition (2026-04-17)

- **15.1** `tirami-economics` README rewrite (139→96 lines):
  "GPU Airbnb × AI Agent Economy". New chapters 15 (hybrid chain)
  and 16 (agent economy). `spec/parameters.md` §20-§21.
- **15.2** `tirami start` one-command bootstrap: auto keygen + model
  download + welcome loan + API in ~30 seconds.
- **15.3** FLOP measurement: `tirami_core::MeterReading`,
  `ModelManifest::flops_per_token()`, `TradeRecord::flops_estimated`.
  Anchors principle 1 "1 TRM = 10⁹ FLOP" in measured data.

### Phase 16 — tirami-anchor crate (skeleton, 2026-04-17)

- New `tirami-anchor` crate (15th in workspace).
- `ChainClient` trait + `MockChainClient`.
- `Anchorer<C>` periodic batcher (default 10 min, 10 k trades/batch).
- `BatchDeltas` / `NodeDelta` payload structs.
- Full daemon integration deferred to Phase F.

### SDK + MCP bindings

- `tirami-sdk`: `peers()`, `schedule()`, `chat_as()`, new types.
- `tirami-mcp`: `tirami_peers`, `tirami_schedule`, `tirami_chat_as`.
  Tool count 40 → 43.

### Aggregate

- **Tests: 785 → 877 passing** (+92).
- **verify-impl.sh**: 123/123 GREEN.
- **E2E verified** on 2-node setup — see `docs/e2e-demo-phase-15.md`.

### Deferred to future phases

- Phase 13 research frontier: real zkML backend (ezkl or risc0), real
  BitVM covenants, real federated training backend, forge-mesh full
  sync (ledger.rs 3-way merge, streaming + tools port)
- Phase E: full audit challenge-response loop (deterministic
  `generate_audit()` + challenger/responder daemon tasks)
- Phase F: tirami-anchor daemon integration + tirami-contracts
  (Solidity + Foundry) + Base L2 deployment
- Crates.io publish after `tirami-core` / `tirami-cli` name rename
- Docker image + Homebrew tap
- Structured docs hosting (ReadTheDocs or Sphinx)

## [0.3.0] - 2026-04-10

**Codename: Launch-ready.** First release verified end-to-end on real
hardware with a real GGUF model. 426 tests passing (Rust), 27 pytest
(Python SDK), 16 SPEC-AUDIT (forge-economics), 686 tests (forge-mesh).
95/95 `verify-impl.sh` assertions GREEN. Theory ↔ implementation audit:
43 match / 0 drift.

### Added

#### Phase 9 — Production hardening (2026-04-08)

- **A1 forge-mesh sync**: 45 new `/api/forge/*` endpoints ported to the
  production mesh-llm runtime. 393 → 641 tests in forge-mesh.
- **A2 persistent L2/L3/L4 state**: `BankServices`, `Marketplace`, and
  `TiramiMindAgent` survive node restarts via JSON snapshots. New
  `StrategyKind` enum and `MindAgentSnapshot` handle trait-object
  fields. New `state_persist.rs` module.
  `POST /v1/tirami/admin/save-state` admin endpoint.
- **A3 reputation gossip**: `ReputationObservation` wire message with
  weighted-median `consensus_reputation()` merge. Outlier-resistant.
- **A4 NIP-90 Nostr relay publish**: `tirami_ledger::agora_relay` with
  `tokio-tungstenite` WebSocket publisher.
- **A5 collusion resistance**: `tirami_ledger::collusion` with tight
  cluster + volume spike + round-robin (Tarjan SCC) detection. Trust
  penalty up to 0.5 subtracted in `effective_reputation()`.
  `/v1/tirami/collusion/{hex}` debug endpoint.
- **B1 tirami-sdk v0.3.0**: 20 new Python methods for all Phase 8
  L2/L3/L4 endpoints. 27 pytest tests.
- **B2 forge-cu-mcp v0.3.0**: 20 new MCP tools for Claude Code /
  Cursor / ChatGPT desktop.
- **Theory audit**: `docs/THEORY-AUDIT.md` — zero drift between
  `parameters.md` and the Rust implementation.

#### Phase 10 — Productization (2026-04-08)

- **P1**: tirami-sdk 0.3.0 + forge-cu-mcp 0.3.0 wheels built and
  twine-checked.
- **P2 Ed25519-signed ReputationObservation**: real cryptographic
  signatures replace the Phase 9 A3 placeholder. Strict verify()
  rejects empty/wrong-length/tampered signatures end-to-end.
- **P3 forge-mesh GitHub Actions CI**: automated cargo check + test on
  every push/PR.
- **P4 forge-mesh persistent state**: state_persist ported to
  forge-mesh with round-trip tests.
- **P5 Prometheus `/metrics`**: `tirami_ledger::metrics` with 11 series
  (cu_contributed, cu_consumed, reputation, trade_count, pool_*,
  collusion_*). Rate-limit-bypassed for scraping.
- **P6 Bitcoin OP_RETURN anchoring**: 40-byte "FRGE" payload in
  `tirami_ledger::anchor`, builds a fully-signable Transaction
  skeleton. `GET /v1/tirami/anchor?network=<...>` endpoint.
- **P7 Compute Standard paper**:
  `forge-economics/papers/compute-standard.md` — 7,031-word academic
  preprint with 20 citations.

#### Phase 11 — Drop-in compatibility (2026-04-09)

- **`forge node -m <name>` auto-resolve**: fixed regression that
  required both `--model` and `--tokenizer`. Now matches `forge chat`
  behavior.
- **Real token-by-token streaming**: the SSE handler no longer buffers
  the entire completion. `tokio::task::spawn_blocking` + channel-based
  `generate_streaming()` delivers per-token chunks with ~2-4 ms
  inter-arrival latency on Apple Silicon Metal.
- **`top_p` / `top_k` sampling**: now honored via
  `LlamaSampler::chain_simple`. Previously `top_p` was parsed but
  ignored; `top_k` did not exist.
- **Accurate prompt token counts**: replaced the `len/4` character
  estimate with real `engine.tokenize(&prompt).len()`.
- **Model name fallback**: `"forge-model"` → `"forge-no-model"` so
  clients can distinguish "loaded but unnamed" from "no model".
- **`docs/compatibility.md`**: 350-line feature matrix vs llama.cpp /
  mesh-llm / Ollama / Bittensor / Akash / Together.ai, per-ecosystem
  migration guides, verified end-to-end transcript.
- **`scripts/demo-e2e.sh`**: one-command end-to-end demo.
- **`docs/hn-teaser-draft.md`**: HN / X / Reddit launch post drafts.
- **tirami-infer unified resolver**: ported from mesh-llm. Supports
  local files, HF full URLs, HF shorthand (`org/repo/file.gguf`),
  catalog names, and `~/.models` scan via a 5-priority dispatcher.

#### Phase 12 — Research-frontier scaffolds (2026-04-09)

- **A1 OpenAI tools / function calling**: `OpenAIChatRequest.tools`
  and `tool_choice` fields. Model-agnostic
  `<tool_call>{...}</tool_call>` injection + extraction. Sync and
  streaming paths both emit `finish_reason: "tool_calls"` when the
  model complies.
- **A2 zkML verification scaffold**: `tirami_ledger::zk` module with
  `ProofVerifier` trait, `ProofOfInference` struct, `MockVerifier`,
  and `VerifierRegistry`. Forward-compatible with ezkl / risc0 /
  halo2 for Phase 13.
- **A3 federated training scaffold**: `tirami_mind::federated` module
  with `GradientContribution`, `FederatedRound`, `Aggregator` trait,
  and `WeightedAverageAggregator`.
- **A4 BitVM optimistic verification scaffold**:
  `tirami_ledger::bitvm` with `StakedClaim`, `FraudProof`,
  `FraudProofVerifier` trait, and a ~1100-word design document at
  `docs/bitvm-design.md`.

#### Phase 12.5 — Launch preparation (2026-04-10)

- **OSS meta-files**: `LICENSE` (MIT), `CONTRIBUTING.md`,
  `SECURITY.md`, `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1).
- **User documentation**:
  - `docs/operator-guide.md` (1,990 words) — production deployment
  - `docs/developer-guide.md` (1,345 words) — contributor onboarding
  - `docs/faq.md` (1,564 words) — 12 Q&A entries
  - `docs/migration-guide.md` (1,209 words) — per-ecosystem migration
- **English theory translations**: `forge-economics/docs/en/` with
  five core chapter translations (00-introduction, 02-money,
  03-supply-demand, 05-banking, 14-programmable-money).
- **Deployment report**:
  `forge-economics/papers/forge-v0.3-deployment.md` — 2,919-word
  companion paper documenting the 2026-04-09 empirical run.
- **LAUNCH.md**: user-gated launch runbook with exact commands for
  PyPI upload, GitHub release, arXiv submission, and community
  post distribution.
- **GitHub community infrastructure**: `.github/workflows/ci.yml`,
  issue templates (bug / feature / question), PR template.

### Changed

- Workspace version bumped from `0.2.0` to `0.3.0`.
- Workspace `repository` URL corrected from `nicola-pache/forge` to
  `clearclown/forge`.
- Workspace metadata adds `homepage`, `documentation`, `authors`,
  `readme` fields for crates.io discoverability.
- Every crate's `Cargo.toml` now inherits metadata from the workspace
  (`repository.workspace = true`, etc.).
- `pyproject.toml` for both tirami-sdk and forge-cu-mcp upgraded to
  `Development Status :: 4 - Beta` with full URLs and classifiers.
- README.md Quick Start restructured: `bash scripts/demo-e2e.sh`
  promoted to Option 1, Python / Rust / Docker as Options 2-4.
- `MetaOptimizer` trait migrated from sync to `#[async_trait]` to
  support reqwest-backed `CuPaidOptimizer`. All 53 tirami-mind tests
  migrated to `#[tokio::test]`.
- CHANGELOG.md expanded from a 781-byte stub to cover all of Phase
  1-12 in Keep-a-Changelog format.

### Fixed

- Streaming endpoint TRM accounting: trade record is now correctly
  written after the stream completes with the actual token count.
- `DEFAULT_REPUTATION = 0.5` hoisted from 11 hardcoded literals in
  `ledger.rs` to a named constant in `lending.rs`. Matches spec §7.
- `HighYieldStrategy::default().base_commit_fraction` corrected from
  0.70 to 0.50 per spec §10.2.
- `RiskModel::default()` corrected from `(0.01, 0.67, 2.33)` to
  `(0.02, 0.50, 2.33)` per spec §10.5.
- `EMA_ALPHA = 0.3` hoisted to a named constant matching spec §2.
- `CONSISTENCY_MIN_TRADES = 2` hoisted to
  `ReputationCalculator::CONSISTENCY_MIN_TRADES` matching spec §12.2.

### Notes

- **crates.io publish is deferred** to Phase 13. The `tirami-core`
  and `tirami-cli` crate names on crates.io are squatted by unrelated
  parties (tirami-core at 0.8.3, tirami-cli at 0.0.0). A rename pass
  will land in Phase 13. In the meantime, install via
  `cargo install --git https://github.com/clearclown/forge tirami-cli`.
- **Demo GIF** is not included in this release. `vhs` and `asciinema`
  were not available on the build host. The `docs/compatibility.md`
  transcript stands in for a visual asset until v0.3.1.

### Empirical validation (2026-04-09)

```text
$ bash scripts/demo-e2e.sh
✓ node PID 26860, model loaded after 1×2s
✓ 3 real chat completions via SmolLM2-135M on Apple Silicon Metal
✓ balance: contributed=41 CU, reputation=0.5 (DEFAULT_REPUTATION)
✓ PortfolioManager.tick() → action=lend
✓ RiskModel VaR 99%: 692 TRM (DEFAULT_RATE=0.02, LGD=0.50, σ=2.33)
✓ Marketplace.find() returned 1 matches
✓ TiramiMindAgent initialized with EchoMetaOptimizer
✓ improve(1) → decision=Revert (correct)
✓ trust_penalty=0.0 (below MIN_TRADES_FOR_ANALYSIS)
✓ forge_trade_count_total 3
✓ merkle_root: 094f69461a6b339c75b7455b90c2c146943261c41f9023173fb142a91864ffb1
✓ Bitcoin OP_RETURN payload: 6a284652474501000000094f69461a6b339c75b7455b90c2c146943261c41f9023173fb142a91864ffb1
```

All Phase 1-12 endpoints verified with live data.

## [0.2.0-alpha] - 2026-04-07

- Phase 8: L2/L3/L4 Rust crates (tirami-bank, tirami-mind, tirami-agora)
  wired into tirami-node with 20 new HTTP endpoints.
- Phase 7: Rust rewrite of tirami-bank, tirami-mind, tirami-agora from
  the original Python scaffolds. Bit-for-bit semantic preservation.
- Phase 6: Multi-model pricing tiers (Small / Medium / Large /
  Frontier) and reputation-adjusted routing.
- Phase 5.5: TRM lending primitives, dual-signed loan records, credit
  scoring, lending pool with circuit breakers.
- Phase 5: Lightning bridge (CU ↔ BTC settlement), Lightning wallet
  CLI, settlement statement export.

## [0.1.0] - 2026-04-02

Initial public MVP prerelease of Tirami.

- Encrypted seed/worker inference over iroh QUIC with Noise handshake
- Loopback-first HTTP API with optional bearer token protection
- Local CU-native ledger, persisted snapshots, settlement export
- Capability handshake, topology planning groundwork, protocol
  hardening

Known boundary at v0.1.0:

- Split inference was target architecture, not the active runtime
  path.
- `Forward` messages and topology planning existed, but real
  multi-stage execution was not shipped.
- Stable release work started from this baseline.

[Unreleased]: https://github.com/clearclown/forge/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/clearclown/forge/compare/v0.2.0-alpha...v0.3.0
[0.2.0-alpha]: https://github.com/clearclown/forge/compare/v0.1.0...v0.2.0-alpha
[0.1.0]: https://github.com/clearclown/forge/releases/tag/v0.1.0
