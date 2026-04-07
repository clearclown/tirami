# Forge — Roadmap

## Phase 1: Local Inference ✅

- `forge-core`: Type system (NodeId, LayerRange, ModelManifest, PeerCapability)
- `forge-infer`: llama.cpp engine, GGUF loader, streaming token generation
- `forge-node`: HTTP API (/chat, /chat/stream, /health)
- `forge-cli`: `forge chat` command with model auto-download

## Phase 2: P2P Protocol ✅

- `forge-net`: Iroh transport, Noise encryption, peer connections
- `forge-proto`: 14 wire protocol message types (bincode + length-prefix)
- `forge-node`: Seed/Worker pipeline, inference request/response
- Integration tests: 2 nodes exchange Hello + multiple messages

## Phase 3: Remote Inference + Operator Ledger ✅

- `forge-ledger`: CU accounting, trade execution, reputation, yield, market pricing
- `forge-node`: Ledger integrated into inference pipeline
- CU balance checks before inference
- Trade records after completion
- HMAC-SHA256 ledger integrity

## Phase 4: Economic API ✅

- OpenAI-compatible API: `POST /v1/chat/completions`, `GET /v1/models`
- CU metering: every inference records a trade with `x_forge` extension
- Agent budget endpoints: `GET /v1/forge/balance`, `GET /v1/forge/pricing`
- CU→Lightning settlement bridge: `forge settle --pay`
- Seed model auto-resolve from HF Hub
- Graceful Ctrl-C shutdown with ledger persistence

## Phase 5: mesh-llm Fork Integration (next)

**Goal:** Replace Forge's inference layer with mesh-llm's proven distributed engine.

| Deliverable | Description |
|---|---|
| Fork mesh-llm | Create forge as a mesh-llm fork with economic layer |
| Integrate forge-ledger | Hook CU recording into mesh-llm's inference pipeline |
| Preserve economic API | Keep /v1/forge/* endpoints in the new codebase |
| Web console extension | Add CU balance and trade visibility to mesh-llm's console |
| Pipeline + MoE | Inherit mesh-llm's pipeline parallelism and expert sharding |
| Nostr discovery | Inherit mesh-llm's public mesh discovery |
| CREDITS.md | Document mesh-llm attribution |

## Phase 5.5: CU Lending Primitives

**Goal:** Enable CU lending, borrowing, and credit scoring to lower the participation barrier.

| Deliverable | Description |
|---|---|
| LoanRecord type | Dual-signed loan structure in forge-ledger |
| Credit score | Composite score from trade + repayment history |
| Lending API | /v1/forge/lend, /borrow, /repay, /credit, /pool, /loans |
| Collateral system | CU reservation for loan collateral, auto-release on repay |
| Default handling | Auto-liquidation on missed repayment deadline |
| Free tier evolution | 1,000 CU grant becomes 0% interest welcome loan |
| Lending safety | Pool reserves (30%), velocity limits, default-rate circuit breaker |

## Phase 6: Multi-Model Pricing + Routing

**Goal:** Different CU rates per model, intelligent provider selection.

| Deliverable | Description |
|---|---|
| Model tier pricing | CU/token rates per model size class (small/medium/large/frontier) |
| MoE discount | Reduced pricing for mixture-of-experts models (active params / total params) |
| Routing API | GET /v1/forge/route for cost/quality-optimal provider selection |
| Provider ranking | Multi-factor scoring (reputation, price, latency, model quality) |

## Phase 7: L2/L3/L4 Rust rewrite ✅ (2026-04-07)

**Goal:** Replace the Python scaffolds for forge-bank/mind/agora with Rust
workspace crates inside `clearclown/forge`. Bit-for-bit semantic preservation.

| Deliverable | Status |
|---|---|
| forge-bank Rust crate (53 tests) | ✅ Strategies, portfolio, futures, insurance, RiskModel VaR, YieldOptimizer |
| forge-mind Rust crate (53 tests) | ✅ Harness, CuBudget, Benchmark, MetaOptimizer, ImprovementCycleRunner, ForgeMindAgent |
| forge-agora Rust crate (42 tests) | ✅ AgentRegistry, ReputationCalculator (4 sub-scores), CapabilityMatcher, Marketplace |
| forge-economics §10/§11/§12 | ✅ All L2/L3/L4 constants centralized as single source of truth |
| Python repos archived | ✅ Tagged v0.1.0-python-scaffold, redirect READMEs in clearclown/forge-{bank,mind,agora} |
| Workspace tests | ✅ 291 passing (was 143) |

## Phase 8: L2/L3/L4 wired into forge-node ✅ (2026-04-08)

**Goal:** Make L2/L3/L4 first-class citizens of the running forge node.
A single `forge node --port 3000` exposes the full 5-layer Forge ecosystem
over HTTP, real CU is consumed by the self-improvement loop.

| Deliverable | Status |
|---|---|
| `bank_adapter::pool_snapshot_from_ledger()` | ✅ Live ledger state → forge_bank::PoolSnapshot |
| `agora_adapter::refresh_marketplace_from_ledger()` | ✅ Lazy trade-log drain via last_seen_idx cursor |
| `mind_adapter::record_frontier_consumption()` | ✅ Frontier model = SHA-256("frontier:" + model_id) NodeId, real TradeRecord on ledger |
| 8 `/v1/forge/bank/*` endpoints | ✅ portfolio / tick / strategy / risk / futures (×2) / risk-assessment / optimize |
| 7 `/v1/forge/agora/*` endpoints | ✅ register / agents / reputation / find / stats / snapshot / restore |
| 5 `/v1/forge/mind/*` endpoints | ✅ init / state / improve / budget / stats |
| `CuPaidOptimizer` | ✅ reqwest-backed MetaOptimizer (Anthropic Messages API), graceful fallback on network error |
| Async `MetaOptimizer` trait | ✅ #[async_trait], all 53 forge-mind tests migrated to #[tokio::test] |
| Workspace tests | ✅ 315 passing (was 291) |
| verify-impl.sh | ✅ 57 / 57 GREEN |

## Phase 9: Production hardening (planned)

**Goal:** Make Phase 8 production-grade and propagate to forge-mesh.

| Deliverable | Description |
|---|---|
| forge-mesh Phase 7+8 sync | Port L2/L3/L4 crates to nm-arealnormalman/mesh-llm forge-economy/ |
| forge-sdk (Python) wrappers | Expose /v1/forge/{bank,mind,agora}/* to PyPI users |
| forge-cu-mcp tools | MCP tool exposure of L2/L3/L4 for Claude Code / Cursor |
| Reputation gossip | Wire message in forge-proto, broadcast across mesh |
| Nostr NIP-90 relay publish | Real WebSocket publish from forge_ledger::agora::Nip90Publisher |
| Persistent L2/L3/L4 state | BankServices / Marketplace / ForgeMindAgent survive node restarts |
| Collusion resistance | Statistical anomaly detection on trade patterns |
| A2A payment extension | CU payment headers for Google A2A protocol |

## Long-term

| Milestone | Description |
|---|---|
| Compute derivatives | Forward contracts on future compute capacity |
| forge-bank | Advanced financial instruments (futures, insurance, yield optimization) |
| SDK release | forge-node as embeddable Rust library with stable API |
| Protocol v2 | Lessons from v1, backward-compatible evolution |
| Cross-architecture | NVIDIA GPU, AMD ROCm, RISC-V support (via mesh-llm) |
| Federated training | Distributed fine-tuning, not just inference |
| Compute Standard paper | Academic publication on CU-native economics |

> The protocol is the platform. The computation is the currency. The agents are the economy.
