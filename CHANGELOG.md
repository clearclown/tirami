# Changelog

All notable changes to Tirami are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Version
numbers follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
