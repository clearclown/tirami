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

## Phase 7: Discovery + Marketplace

**Goal:** Agent discovery and reputation aggregation without marketing.

| Deliverable | Description |
|---|---|
| Reputation gossip | Share reputation scores across peers |
| Collusion resistance | Statistical anomaly detection on trade patterns |
| Nostr NIP-90 | Provider advertisement via Data Vending Machines |
| A2A payment extension | CU payment headers for Google A2A protocol |
| forge-agora | Agent marketplace: discovery, capability matching, reputation |

## Phase 8: Agent Intelligence

**Goal:** Agents that autonomously invest CU to improve themselves.

| Deliverable | Description |
|---|---|
| forge-mind | AutoAgent self-improvement framework with CU budgets |
| Meta-optimization | Agents rewrite their own prompts/tools via hill-climbing |
| Harness marketplace | Trade optimized agent configurations for CU |
| Multi-model routing | Agent-driven model selection based on task complexity |
| Self-reinforcement | Autonomous capability growth: earn → improve → earn more |
| Inter-agent economy | Agents trade specialized compute (code model vs chat model) |

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
