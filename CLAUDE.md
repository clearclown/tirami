# Tirami — Development Guide

## What This Project Is

Tirami is a distributed LLM inference protocol where **compute is currency**. The inference layer is built on [mesh-llm](https://github.com/michaelneale/mesh-llm). Tirami's original contribution is the **economic layer**: TRM (Tirami Resource Merit) accounting, Proof of Useful Work, dynamic pricing, and autonomous agent budgets.

**Three pillars:**
1. CU-native economy — compute is the currency, not Bitcoin
2. Proof of Useful Work — dual-signed trades, gossip verification
3. Agent autonomy — AI agents manage their own compute budgets

## Repositories

| Repo | Language | Status | Layer | Purpose |
|------|----------|--------|-------|---------|
| `clearclown/tirami` (this) | Rust | Active (1,192 tests, Phase 19) | L1-L4 | Protocol core + finance, intelligence, marketplace + tokenomics + governance (21 mutable / 18 constitutional) + staking + slashing loop + collusion detection + NIP-90 relay + Prometheus metrics + Bitcoin OP_RETURN + hybrid-chain anchor + PeerRegistry/PriceSignal/select_provider + FLOP measurement + `tirami start` + audit challenge-response + dual-signed P2P trade w/ nonce replay protection + PersonalAgent + peer auto-discovery + HTTP→P2P forwarding + gated Base mainnet Makefile (Rust workspace, 16 crates incl. `tirami-zkml-bench` + `tirami-attestation`) |
| `clearclown/tirami-contracts` | Solidity (Foundry) | 15 tests passing | On-chain | TRM ERC-20 + TiramiBridge. Target: Base L2. **Not deployed to mainnet** — `Makefile` gated on `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER` + interactive prompt. Base Sepolia deploy is free and unblocked. |
| `nm-arealnormalman/mesh-llm` | Rust | Active (43 tests) | L0 | mesh-llm + Tirami economy = production runtime |
| `clearclown/tirami-bank` | Python (archived) | Scaffold v0.1 (45 tests) | — | Superseded by `crates/tirami-bank/` in this repo |
| `clearclown/tirami-mind` | Python (archived) | Scaffold v0.1 (40 tests) | — | Superseded by `crates/tirami-mind/` in this repo |
| `clearclown/tirami-agora` | Python (archived) | Scaffold v0.1 (39 tests) | — | Superseded by `crates/tirami-agora/` in this repo |
| `clearclown/forge-economics` | Markdown | Active (16/16 GREEN) | Theory | Economic theory, design rationale, parameters (§1-§12 = single source of truth for all layers) |
| `tirami-sdk` (in-tree) | Rust | Active (15 tests) | Client | Rust async HTTP client for Tirami API |
| `tirami-mcp` (in-tree) | Rust | Active (5 tests) | Client | Rust MCP server (40 tools for Claude/Cursor) |

### 5-Layer Architecture (all layers are Rust since 2026-04-07 Phase 7 — now at Phase 19 as of 2026-04-19)

```
L4: Discovery     crates/tirami-agora          — Agent marketplace, reputation, NIP-90 (42 tests)
L3: Intelligence  crates/tirami-mind           — AutoAgent self-improvement paid in TRM (53 tests)
L2: Finance       crates/tirami-bank           — Strategies, portfolios, futures, insurance (53 tests)
L1: Economy       crates/tirami-ledger et al.  — TRM ledger, trades, lending, safety (143 tests)
L0: Inference     nm-arealnormalman/mesh-llm  — Distributed LLM inference + forge-economy port
```

**Total tests across the ecosystem:** 1,192 (tirami workspace) + 646 (forge-mesh) +
15 (tirami-contracts Foundry) + 16 (tirami-economics SPEC-AUDIT) = **1,869 passing**.

Phase 7 (2026-04-07) rewrote L2/L3/L4 from Python scaffolds into Rust
workspace crates. Phase 8 (2026-04-08) wired them into tirami-node with
20 new HTTP endpoints (8 bank + 7 agora + 5 mind), plus a CuPaidOptimizer
that calls a frontier LLM via reqwest and records the TRM consumption as
a real TradeRecord on the ledger. A single `forge node --port 3000` now
exposes the full 5-layer Tirami ecosystem.

All L2/L3/L4 numeric constants reference `forge-economics/spec/parameters.md`
§10/§11/§12 as the single source of truth — no re-definition in Rust code.

The integrated fork at `/Users/ablaze/Projects/forge-mesh` contains mesh-llm's full distributed inference engine with Tirami's economic crates (`forge-economy/`) and API routes (`/api/forge/*`).

## Build & Test

```bash
cargo build --release          # Full build
cargo test --workspace         # All tests (891 across 15 crates)
cargo check --workspace        # Fast type check
cargo clippy --workspace       # Lint
```

Rust edition 2024, resolver v2. Apple Silicon Metal enabled by default for inference.

## Architecture: Two Layers

```
Economic Layer (Tirami-original)    ← This is what we build
├── tirami-ledger   TRM trades, pricing, yield, settlement
├── tirami-lightning CU↔BTC bridge (optional)
├── tirami-node/api OpenAI API + /v1/tirami/* economic endpoints
└── forge-verify   (planned) dual-sign, gossip, PoUW

Inference Layer (mesh-llm-derived)  ← This is inherited
├── tirami-net      iroh QUIC + Noise encryption
├── tirami-infer    llama.cpp backend
├── tirami-proto    wire protocol (bincode, 14 message types)
└── forge-shard    layer assignment
```

**When making changes, prioritize the economic layer.** Inference/networking code will eventually be replaced by mesh-llm's implementation.

## Crate Map

| Crate | Lines | Role | Priority |
|-------|-------|------|----------|
| `tirami-ledger` | ~770 | **Core economic engine** — trades, pricing, yield | Highest |
| `tirami-node` | ~2500 | Daemon, HTTP API, pipeline coordinator | High |
| `tirami-lightning` | ~330 | CU↔Bitcoin Lightning bridge | Medium |
| `tirami-proto` | ~430 | Wire protocol messages | Medium |
| `tirami-core` | ~330 | Shared types: NodeId, CU, Config | Medium |
| `tirami-cli` | ~1050 | Reference CLI (chat, seed, worker, su) | Low (will change with mesh-llm fork) |
| `tirami-net` | ~1400 | P2P transport | Low (replaced by mesh-llm) |
| `tirami-infer` | ~1270 | llama.cpp inference | Low (replaced by mesh-llm) |
| `forge-shard` | ~130 | Topology planner | Low (replaced by mesh-llm) |

## Key Design Rules

1. **CU is the native currency.** Bitcoin/Lightning is an optional off-ramp, not the foundation. Never make Bitcoin a hard dependency in the economic engine.

2. **Trades must be bilateral.** Every TRM transfer has a provider (earns) and consumer (spends). Target: both parties sign. Current: local ledger only.

3. **The protocol settles in CU.** External bridges (Lightning, stablecoin, fiat) are adapters outside the core protocol. Settlement endpoint exports data; it does not execute payments.

4. **No blockchain in the core.** TRM accounting uses local ledgers + gossip + dual signatures. Bitcoin anchoring is optional and future.

5. **No tokens, no ICO.** TRM is earned by performing useful computation, not purchased or speculated on.

6. **Agent-first API.** The `/v1/tirami/balance` and `/v1/tirami/pricing` endpoints exist so AI agents can make autonomous economic decisions. Design APIs that machines can use without human help.

7. **Loans are bilateral.** Every loan requires dual signatures (lender + borrower). No unilateral lending. LoanRecords follow the same dual-sign + gossip pattern as TradeRecords.

8. **Credit scores are local-first.** Each node computes credit scores from its own observed trade and repayment history. No central credit bureau.

9. **Lending has circuit breakers.** Pool reserves (30% minimum), velocity limits, and default-rate triggers prevent cascading failures. Fail-safe: if uncertain, deny the loan.

## Code Conventions

- Error handling: `TiramiError` enum in tirami-core, `anyhow` in CLI only
- Serialization: `serde` for JSON/config, `bincode` for wire protocol
- Async: `tokio` runtime, `Arc<Mutex<T>>` for shared state
- Logging: `tracing` crate, INFO for user-visible events, DEBUG for protocol details
- Tests: Unit tests in each module, integration tests in `tests/` dirs
- Security: HMAC-SHA256 for ledger integrity, Noise protocol for transport, constant-time comparison for auth tokens

## API Surface

### OpenAI-Compatible (inherited from inference layer)
- `POST /v1/chat/completions` — Chat with streaming, includes `x_tirami.trm_cost`. Auto-forwards to a connected peer via `forward_chat_to_peer` if no local model is loaded (Phase 19).
- `GET /v1/models` — List loaded models

### Tirami Economic (our original contribution)
- `GET /v1/tirami/balance` — TRM balance, reputation, contribution history
- `GET /v1/tirami/pricing` — Market price (EMA smoothed), supply/demand, cost estimates
- `GET /v1/tirami/trades` — Recent trade history (provider, consumer, CU, tokens)
- `GET /v1/tirami/network` — Mesh economic summary + Merkle root
- `GET /v1/tirami/providers` — Ranked providers with reputation-adjusted costs (agent routing)
- `POST /v1/tirami/invoice` — Create Lightning invoice from TRM balance
- `GET /status` — Node health, market price, recent trades
- `GET /settlement` — Exportable settlement statement with Merkle root
- `GET /topology` — Model manifest, peer capabilities

### Tirami Lending (Phase 5.5 — implemented)
- `POST /v1/tirami/lend` — Offer TRM to lending pool
- `POST /v1/tirami/borrow` — Request a TRM loan
- `POST /v1/tirami/lend-to` — Lender-initiated loan proposal to a specific borrower
- `POST /v1/tirami/repay` — Repay outstanding loan
- `GET /v1/tirami/credit` — Credit score and history
- `GET /v1/tirami/pool` — Lending pool status (available, utilization, avg rate, your max borrow)
- `GET /v1/tirami/loans` — Active loans (as lender or borrower)

### Tirami Routing (Phase 6 — implemented)
- `GET /v1/tirami/route?model=X&max_cu=Y&mode=cost|quality|balanced` — Optimal provider selection

### Tirami Unified Scheduler (Phase 14 — implemented)
- `GET /v1/tirami/peers` — PeerRegistry dump (price_multiplier, available_cu, audit_tier, latency_ema_ms, models)
- `POST /v1/tirami/schedule` — Ledger-as-Brain probe. `{model_id, max_tokens, consumer?}` → `{provider, estimated_trm_cost}` (read-only, no TRM reserved)
- Chat completions now attribute trades via `X-Tirami-Node-Id` header (Phase 14.3) and record `flops_estimated` on every `TradeRecord` (Phase 15)

### Tirami Hybrid Chain Anchor (Phase 16 — implemented, MockChainClient default)
- `GET /v1/tirami/anchors` — list submitted batches: `batch_id`, `tx_hash`, `merkle_root_hex`, `submitted_at_ms`, `node_count`, `flops_total`
- Anchor loop runs every `config.anchor_interval_secs` (default 3600 dev, 600 prod per §20)
- Swappable `ChainClient` trait — `MockChainClient` in-memory default; future `BaseClient` for Base L2

### Tirami Bank L2 (Phase 8 — implemented)
- `GET /v1/tirami/bank/portfolio` — Portfolio snapshot + cash/lent/borrowed/exposure
- `POST /v1/tirami/bank/tick` — Run PortfolioManager.tick() with live PoolSnapshot from ledger
- `POST /v1/tirami/bank/strategy` — Hot-swap strategy (conservative / highyield / balanced)
- `POST /v1/tirami/bank/risk` — Set RiskTolerance
- `GET /v1/tirami/bank/futures` — List FuturesContracts
- `POST /v1/tirami/bank/futures` — Create a FuturesContract
- `GET /v1/tirami/bank/risk-assessment` — RiskModel VaR 99% on current portfolio
- `POST /v1/tirami/bank/optimize` — YieldOptimizer with VaR cap

### Tirami Agora L4 (Phase 8 — implemented)
- `POST /v1/tirami/agora/register` — Register an AgentProfile
- `GET /v1/tirami/agora/agents` — List registered agents
- `GET /v1/tirami/agora/reputation/{hex}` — ReputationScore (lazy-refreshes from ledger trade log)
- `POST /v1/tirami/agora/find` — CapabilityQuery → ranked CapabilityMatches
- `GET /v1/tirami/agora/stats` — Marketplace stats
- `GET /v1/tirami/agora/snapshot` — Serialize RegistrySnapshot for backup
- `POST /v1/tirami/agora/restore` — Restore from RegistrySnapshot

### Tirami Mind L3 (Phase 8 — implemented)
- `POST /v1/tirami/mind/init` — Initialize ForgeMindAgent (echo / prompt_rewrite / cu_paid optimizer)
- `GET /v1/tirami/mind/state` — Harness summary + cycle history + budget remaining
- `POST /v1/tirami/mind/improve` — Run N improvement cycles; TRM is deducted from ledger when CuPaidOptimizer is active
- `POST /v1/tirami/mind/budget` — Update CuBudget hard limits (per-cycle / per-day / cycles-per-day)
- `GET /v1/tirami/mind/stats` — kept / reverted / deferred counts + total TRM invested

All `/v1/tirami/*` endpoints are rate-limited (token bucket, 30 req/sec).

## What's Implemented vs Planned

### Phase 17-19 — Hardening + mainnet gate (DONE 2026-04-19, 1,192 tests)

**Phase 17 Wave 1-3 — Hostile-network hardening:**
- Wave 1.3: `slashing::SlashingEngine` + automatic slashing loop inside `tirami-node` (interval `slashing_interval_secs`). Collusion detector + audit-tier failures → slashing events recorded on ledger.
- Wave 3.1: `tirami-attestation` crate — scaffold for Apple Secure Enclave / NVIDIA H100 CC TEE attestation (not wired; Phase 20+).
- Wave 3.2: Kani formal-verification harness (10 initial invariants over ledger).
- Wave 3.4: DDoS mitigation — `max_concurrent_connections` cap + per-ASN rate limits.
- Wave 3.5: Key-rotation scaffold for node identities.
- Wave 3.6: Bug-bounty framework (`SECURITY.md` with placeholder PGP key; program not live).

**Phase 18 — Governance + sunset:**
- 18.1 Constitution: `IMMUTABLE_CONSTITUTIONAL_PARAMETERS` (18 entries: `TOTAL_TRM_SUPPLY=21B`, `FLOPS_PER_CU=1e9`, `SLASH_RATE_*`, `PROOF_POLICY_RATCHET`, `WELCOME_LOAN_SUNSET_EPOCH=2`, `CANONICAL_BYTES_V2`, `SIGNATURE_SCHEME_BASE=Ed25519`, ...) and `MUTABLE_GOVERNANCE_PARAMETERS` (21 entries). `create_proposal` auto-rejects names outside the mutable list.
- 18.2 Stake-required mining scaffold (`can_provide_inference` implemented; **not yet enforced** in HTTP/P2P trade path).
- 18.5 `PersonalAgent` + `RunRemote` HTTP dispatch + `tirami agent chat` CLI.

**Phase 19 — Tier C/D enablers (peer auto-discovery + mainnet gate):**
- Peer HTTP auto-discovery via `PriceSignal.http_endpoint` on the gossip stream.
- `forward_chat_to_peer` — worker with no local model forwards `/v1/chat/completions` to a seed.
- `ProofPolicy::default() = Optional` (single-source-of-truth at enum level; Config string default matches).
- `tirami-zkml-bench` crate — `MockBackend` only; real `ezkl` / `risc0` backends in Phase 20+.
- `repos/tirami-contracts/Makefile` — 3-gate mainnet deploy (`AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER` + interactive prompt). Base Sepolia deploy is free and ungated.
- Whitepaper, release-readiness, constitution, killer-app, zkml-strategy docs under `docs/`.

**Status Honesty baseline for the public README**:
- ✅ 14 Functional-today items (dual-signed P2P trade, slashing loop, governance whitelist, welcome loan, stake pool, referral, anchors, Base Sepolia contracts, `PersonalAgent`, HTTP→P2P forwarding, peer auto-discovery, collusion detection, Prometheus, nonce replay protection).
- 🟡 5 Scaffolded (zkML MockBackend, ML-DSA PQ hybrid, TEE attestation, worker gossip-recv loop #88, stake-required mining enforcement).
- ❌ 3 Not done (external security audit, live bug-bounty w/ real PGP, ≥ 30-day Sepolia stable + ≥ 7-day 10-node stress test).
- On **Base L2 mainnet**: maintainers do not plan, operate, or track any mainnet deploy of TRM / TiramiBridge. The `make deploy-base-mainnet` Makefile gate is a self-protective check for any operator who chooses to deploy, not a maintainer-authorization switch. MIT OSS means third parties technically can; they do so entirely on their own account. See `SECURITY.md § Secondary Markets`.

### Phase 10 — Productization (DONE 2026-04-09, 359 tests)
- **P1 PyPI release artifacts**: tirami-sdk 0.3.0 + forge-cu-mcp 0.3.0 wheels built, twine-checked, git-tagged. User executes `twine upload` when ready (PyPI credentials required). Release checklist at `sdk/python/PUBLISH-0.3.0.md`.
- **P2 Ed25519 signed reputation gossip**: `ReputationObservation::new_signed()` replaces the Phase 9 A3 placeholder. Strict verify() rejects empty/wrong-length/tampered sigs. Rejection propagated end-to-end (proto → net → ledger): unsigned observations cannot touch `remote_reputation` or influence consensus.
- **P3 forge-mesh GitHub Actions CI**: `.github/workflows/rust-workspace.yml` runs cargo check + test on every push/PR. README badge added.
- **P4 forge-mesh persistent L2/L3/L4 state**: `mesh-llm/src/api/routes/state_persist.rs` ported from forge Phase 9 A2. ForgeEconomy extended with bank/marketplace/mind paths + `save_state()` + `POST /api/forge/admin/save-state` endpoint. +5 round-trip tests.
- **P5 Prometheus / OpenMetrics export**: `tirami_ledger::metrics::ForgeMetrics` with 11 metric series (cu_contributed, cu_consumed, reputation, trade_count, pool_*, collusion_*). `GET /metrics` endpoint on tirami-node lazily observes ledger state and encodes OpenMetrics text. Rate-limit-bypassed for Prometheus scraping.
- **P6 Bitcoin OP_RETURN anchoring**: `tirami_ledger::anchor` module builds 40-byte anchor payloads (magic "FRGE" + version + network + reserved + 32-byte Merkle root) and fully-signable `Transaction` skeletons. `GET /v1/tirami/anchor?network=testnet` endpoint. External wallet adds inputs + signs + broadcasts.
- **P7 Compute Standard paper v0.1**: `forge-economics/papers/compute-standard.md` — 7,000-word academic preprint synthesizing the theory (docs/00-14 + spec/parameters.md) and the empirical Phase 1-10 results. 13 sections + 2 appendices. Ready for arXiv.

### Phase 9 — Production hardening (DONE 2026-04-08, 337 tests)
- **Theory audit**: 3 drifts + 1 missing + 2 implicit constants fixed; Rust now 1:1 with forge-economics §1-§12 (43 match / 0 drift). See `docs/THEORY-AUDIT.md`.
- **A1 forge-mesh sync**: full Phase 7+8 port into nm-arealnormalman/mesh-llm; 45 new /api/forge/* endpoints + 3 L2/L3/L4 crates + 3 missing tirami-ledger modules (agentnet, agora, safety). forge-mesh test count: 393 → 641.
- **A2 Persistent L2/L3/L4 state**: BankServices / Marketplace / ForgeMindAgent survive node restarts via JSON snapshots. Trait-object fields (Strategy, MetaOptimizer, Benchmark) handled via kind-enum snapshots + re-attachment on load. New `state_persist.rs` module, `POST /v1/tirami/admin/save-state` admin endpoint.
- **A3 Reputation gossip**: `ReputationObservation` wire message + `broadcast_reputation`/`handle_reputation_gossip` + `consensus_reputation()` weighted-median merge on ComputeLedger. Decentralized reputation consensus resistant to single-observer bias.
- **A4 NIP-90 relay publish**: tokio-tungstenite WebSocket publisher in `tirami_ledger::agora_relay`. `Nip90Publisher::publish_advertisement()` actually reaches wss://relay.damus.io.
- **A5 Collusion resistance**: `tirami_ledger::collusion::CollusionDetector` with tight-cluster + volume-spike + round-robin Tarjan-SCC detection. `ComputeLedger::effective_reputation()` subtracts the trust penalty. New `/v1/tirami/collusion/{hex}` debug endpoint.
- **B1 tirami-sdk v0.3.0**: 20 new Python methods (bank 8 + agora 7 + mind 5) + 27 pytest tests.
- **B2 forge-cu-mcp v0.3.0**: 20 new MCP tools exposing L2/L3/L4 to Claude Code / Cursor / ChatGPT desktop.

### Phase 8 — L2/L3/L4 wired into tirami-node (DONE 2026-04-08, 315 tests)
- **tirami-bank as a service**: PortfolioManager owned by ForgeNode, fed live PoolSnapshot from ComputeLedger via `bank_adapter::pool_snapshot_from_ledger()`. 8 HTTP endpoints under `/v1/tirami/bank/*`.
- **tirami-agora as a service**: Marketplace owned by ForgeNode, lazy-refreshes from the ledger trade log on each `/agora/*` request via `agora_adapter::refresh_marketplace_from_ledger()` with a `last_seen_idx` cursor. 7 HTTP endpoints under `/v1/tirami/agora/*`.
- **tirami-mind as a service**: ForgeMindAgent (opt-in) owned by ForgeNode. 5 HTTP endpoints under `/v1/tirami/mind/*`.
- **CuPaidOptimizer**: tirami-mind MetaOptimizer that calls a frontier LLM via reqwest (Anthropic Messages API shape). On `/improve`, the tirami-node handler records each cycle's `cu_cost_to_propose` as a real `TradeRecord` on the ledger via `mind_adapter::record_frontier_consumption()`. The frontier model is identified by `frontier_node_id(model_id) = SHA-256("frontier:" + model_id)`. TRM is actually deducted.
- **Async MetaOptimizer trait**: tirami-mind migrated to `#[async_trait]` so CuPaidOptimizer can `.await` reqwest. EchoMetaOptimizer / PromptRewriteOptimizer adapted as no-op async impls. All 53 tirami-mind tests migrated to `#[tokio::test]`.

### Historical foundation (Phase 1-6 — now subsumed into Phase 7-19)
- TRM ledger with HMAC-SHA256 persistence and tamper detection
- **Dual-signed trades** (Ed25519): TradeProposal → TradeAccept → SignedTradeRecord
- **Dual-signed loans** (Ed25519): LoanProposal → LoanAccept → SignedLoanRecord
- **Gossip protocol**: signed trades AND loans broadcast to all peers with dedup (broadcast_loan / handle_loan_gossip)
- **CU reservation**: reserve before inference or as collateral, release on failure
- Dynamic market pricing (supply/demand)
- **Multi-model pricing tiers** (Phase 6): Small/Medium/Large/Frontier with MoE discount
- Free tier (1,000 CU) with Sybil protection (>100 unknown nodes → reject)
- Reputation system with yield (0.1%/hr × reputation)
- **CU lending** (Phase 5.5): LoanRecord, credit score (0.3*trade + 0.4*repayment + 0.2*uptime + 0.1*age),
  lending pool with 30% reserve / 3:1 max LTV / 20% max single loan, default circuit breaker
- **Lending safety** (Phase 5.5): LendingCircuitState with velocity limit (10/min), default rate threshold (10%/hr)
- **Welcome loan**: 1,000 TRM at 0% interest, 72hr term (replaces flat free tier grant)
- OpenAI-compatible API with TRM metering (`x_tirami.trm_cost` extension field)
- **Lending API** (7 endpoints): `/v1/tirami/lend`, `/borrow`, `/lend-to`, `/repay`, `/credit`, `/pool`, `/loans`
- **Routing API** (Phase 6): `/v1/tirami/route` with cost/quality/balanced modes
- Agent budget endpoints (`/v1/tirami/balance`, `/pricing`, `/trades`, `/providers`)
- **Bidirectional Lightning bridge**: `POST /v1/tirami/invoice` (CU→BTC) + `create_deposit()` (BTC→CU)
- Lightning wallet (CLI: `forge wallet`, `forge settle --pay`)
- Settlement statement export
- P2P encrypted transport (iroh QUIC + Noise)
- **NIP-90 (Data Vending Machines) scaffold**: `tirami_ledger::agora::Nip90Publisher` builds well-formed
  kind 5050/6050/31990 events for future Nostr relay integration
- **forge-mesh fork synced**: Phase 5.5+ ported to forge-mesh/forge-economy/ (production runtime)
- **Python SDK**: `forge_sdk` with full lending coverage (lend, borrow, repay, credit, pool, loans, route)
- **MCP server**: 7 lending tools exposed to Claude/ChatGPT/Cursor

### Sister repositories (all Layer 2-4 scaffolds exist as v0.1)

- **tirami-bank** (L2): registry, strategies, portfolio manager, futures, insurance, risk
  model, yield optimizer with risk-budget gate. Pluggable strategies (Conservative,
  HighYield, Balanced). 45 tests.
- **tirami-mind** (L3): Harness with monotonic versioning, CUBudget with hard limits,
  Benchmark / MetaOptimizer / ImprovementCycleRunner / ForgeMindAgent autonomous loop.
  Stub optimizers (Echo, PromptRewrite); CUPaidOptimizer planned for v0.2. 40 tests.
- **tirami-agora** (L4): AgentRegistry, ReputationCalculator (volume/recency/diversity/
  consistency), CapabilityMatcher with composite scoring, Marketplace facade. 39 tests.

### Phase 7+ work (cross-repo)
- Live tirami-sdk feed in tirami-agora (real /v1/tirami/trades polling)
- CUPaidOptimizer in tirami-mind (real frontier model proposals via tirami-sdk)
- tirami-bank → tirami-sdk integration (real lend/borrow execution)
- Nostr NIP-90 relay submission from tirami_ledger::agora event builders
- Reputation gossip across the forge mesh
- Merkle tree of trade history for efficient state comparison
- Bitcoin OP_RETURN anchoring for immutable audit trail
- Compute Standard academic paper

## Common Tasks

### Adding a new economic endpoint
1. Add handler in `crates/tirami-node/src/api.rs`
2. Add types as needed in the same file
3. Wire into the `protected` router in `create_router()`
4. Add test in the `#[cfg(test)]` block

### Modifying the ledger
1. Edit `crates/tirami-ledger/src/ledger.rs`
2. Add test in the same file's `mod tests`
3. If new fields on `NodeBalance` or `TradeRecord`, update `tirami-core/src/types.rs`
4. Run `cargo test --package tirami-ledger`

### Adding a new wire message
1. Add variant to `Payload` enum in `crates/tirami-proto/src/messages.rs`
2. Add validation in `validate_with_sender()`
3. Add handling in `crates/tirami-net/src/cluster.rs` or `tirami-node/src/pipeline.rs`

## File Locations

- Economic engine: `crates/tirami-ledger/src/ledger.rs`
- HTTP API + economic endpoints: `crates/tirami-node/src/api.rs`
- Core types (NodeId, CU, etc.): `crates/tirami-core/src/types.rs`
- Configuration: `crates/tirami-core/src/config.rs`
- Wire protocol: `crates/tirami-proto/src/messages.rs`
- Lightning bridge: `crates/tirami-lightning/src/payment.rs`
- CLI entry point: `crates/tirami-cli/src/main.rs`
- Node orchestrator: `crates/tirami-node/src/node.rs`
- Pipeline coordinator: `crates/tirami-node/src/pipeline.rs`

## Docs

- `docs/strategy.md` — Competitive positioning, lending spec, 5-layer architecture
- `docs/monetary-theory.md` — Why TRM works: Soddy, Bitcoin, PoUW, AI-only currency thesis
- `docs/concept.md` — Why compute is money, post-marketing economy
- `docs/economy.md` — CU-native economy, Proof of Useful Work, lending
- `docs/architecture.md` — Two-layer design
- `docs/agent-integration.md` — SDK, MCP, borrowing workflow, credit building
- `docs/a2a-payment.md` — TRM payment extension for A2A/MCP
- `docs/protocol-spec.md` — Wire protocol spec
- `docs/roadmap.md` — Development phases (1-19 + long-term)
- `docs/release-readiness.md` — Tier A-D release gates (public OSS → mainnet audit gate)
- `docs/constitution.md` — Governance whitelist + immutable parameters + amendment rules
- `docs/killer-app.md` — PersonalAgent + auto-economy product commitment
- `docs/whitepaper.md` — 16-section protocol spec (production reference)
- `docs/zkml-strategy.md` — Phase 20+ proof-of-inference rollout
- `docs/public-api-surface.md` — Stability boundary for the 5 public crates
- `docs/deployments/README.md` — Base Sepolia / mainnet deploy records
- `SECURITY.md` — Threat disclosure + secondary-market non-involvement stance
- `docs/threat-model.md` — Security + economic threats (T1-T17)
- `docs/bootstrap.md` — Startup, degradation, recovery
- `CREDITS.md` — mesh-llm attribution
