# Forge — Development Guide

## What This Project Is

Forge is a distributed LLM inference protocol where **compute is currency**. The inference layer is built on [mesh-llm](https://github.com/michaelneale/mesh-llm). Forge's original contribution is the **economic layer**: CU (Compute Unit) accounting, Proof of Useful Work, dynamic pricing, and autonomous agent budgets.

**Three pillars:**
1. CU-native economy — compute is the currency, not Bitcoin
2. Proof of Useful Work — dual-signed trades, gossip verification
3. Agent autonomy — AI agents manage their own compute budgets

## Repositories

| Repo | Language | Status | Layer | Purpose |
|------|----------|--------|-------|---------|
| `clearclown/forge` (this) | Rust | Active | L1 | Protocol core: CU ledger, trades, lending, safety |
| `nm-arealnormalman/mesh-llm` | Rust | Active | L0 | mesh-llm + Forge economy = production runtime |
| `forge-sdk` | Python | Published (PyPI) | Client | Python SDK for Forge API |
| `forge-cu-mcp` | Python | Published (PyPI) | Client | MCP server for AI tools |
| `forge-bank` | Rust + Python | Planned | L2 | Advanced financial instruments |
| `forge-mind` | Python | Planned | L3 | AutoAgent self-improvement + CU economy |
| `forge-agora` | Python/TS | Planned | L4 | Agent marketplace, Nostr NIP-90, A2A |

### 5-Layer Architecture

```
L4: Discovery (forge-agora)     — Agent marketplace, reputation, NIP-90
L3: Intelligence (forge-mind)   — AutoAgent self-improvement loops
L2: Finance (forge-bank)        — Advanced CU financial instruments
L1: Economy (forge — this repo) — CU ledger, trades, lending, safety
L0: Inference (forge-mesh)      — Distributed LLM inference
```

The integrated fork at `/Users/ablaze/Projects/forge-mesh` contains mesh-llm's full distributed inference engine with Forge's economic crates (`forge-economy/`) and API routes (`/api/forge/*`).

## Build & Test

```bash
cargo build --release          # Full build
cargo test --workspace         # All tests (~47 tests)
cargo check --workspace        # Fast type check
cargo clippy --workspace       # Lint
```

Rust edition 2024, resolver v2. Apple Silicon Metal enabled by default for inference.

## Architecture: Two Layers

```
Economic Layer (Forge-original)     ← This is what we build
├── forge-ledger   CU trades, pricing, yield, settlement
├── forge-lightning CU↔BTC bridge (optional)
├── forge-node/api OpenAI API + /v1/forge/* economic endpoints
└── forge-verify   (planned) dual-sign, gossip, PoUW

Inference Layer (mesh-llm-derived)  ← This is inherited
├── forge-net      iroh QUIC + Noise encryption
├── forge-infer    llama.cpp backend
├── forge-proto    wire protocol (bincode, 14 message types)
└── forge-shard    layer assignment
```

**When making changes, prioritize the economic layer.** Inference/networking code will eventually be replaced by mesh-llm's implementation.

## Crate Map

| Crate | Lines | Role | Priority |
|-------|-------|------|----------|
| `forge-ledger` | ~770 | **Core economic engine** — trades, pricing, yield | Highest |
| `forge-node` | ~2500 | Daemon, HTTP API, pipeline coordinator | High |
| `forge-lightning` | ~330 | CU↔Bitcoin Lightning bridge | Medium |
| `forge-proto` | ~430 | Wire protocol messages | Medium |
| `forge-core` | ~330 | Shared types: NodeId, CU, Config | Medium |
| `forge-cli` | ~900 | Reference CLI | Low (will change with mesh-llm fork) |
| `forge-net` | ~1400 | P2P transport | Low (replaced by mesh-llm) |
| `forge-infer` | ~1270 | llama.cpp inference | Low (replaced by mesh-llm) |
| `forge-shard` | ~130 | Topology planner | Low (replaced by mesh-llm) |

## Key Design Rules

1. **CU is the native currency.** Bitcoin/Lightning is an optional off-ramp, not the foundation. Never make Bitcoin a hard dependency in the economic engine.

2. **Trades must be bilateral.** Every CU transfer has a provider (earns) and consumer (spends). Target: both parties sign. Current: local ledger only.

3. **The protocol settles in CU.** External bridges (Lightning, stablecoin, fiat) are adapters outside the core protocol. Settlement endpoint exports data; it does not execute payments.

4. **No blockchain in the core.** CU accounting uses local ledgers + gossip + dual signatures. Bitcoin anchoring is optional and future.

5. **No tokens, no ICO.** CU is earned by performing useful computation, not purchased or speculated on.

6. **Agent-first API.** The `/v1/forge/balance` and `/v1/forge/pricing` endpoints exist so AI agents can make autonomous economic decisions. Design APIs that machines can use without human help.

7. **Loans are bilateral.** Every loan requires dual signatures (lender + borrower). No unilateral lending. LoanRecords follow the same dual-sign + gossip pattern as TradeRecords.

8. **Credit scores are local-first.** Each node computes credit scores from its own observed trade and repayment history. No central credit bureau.

9. **Lending has circuit breakers.** Pool reserves (30% minimum), velocity limits, and default-rate triggers prevent cascading failures. Fail-safe: if uncertain, deny the loan.

## Code Conventions

- Error handling: `ForgeError` enum in forge-core, `anyhow` in CLI only
- Serialization: `serde` for JSON/config, `bincode` for wire protocol
- Async: `tokio` runtime, `Arc<Mutex<T>>` for shared state
- Logging: `tracing` crate, INFO for user-visible events, DEBUG for protocol details
- Tests: Unit tests in each module, integration tests in `tests/` dirs
- Security: HMAC-SHA256 for ledger integrity, Noise protocol for transport, constant-time comparison for auth tokens

## API Surface

### OpenAI-Compatible (inherited from inference layer)
- `POST /v1/chat/completions` — Chat with streaming, includes `x_forge` CU cost
- `GET /v1/models` — List loaded models

### Forge Economic (our original contribution)
- `GET /v1/forge/balance` — CU balance, reputation, contribution history
- `GET /v1/forge/pricing` — Market price (EMA smoothed), supply/demand, cost estimates
- `GET /v1/forge/trades` — Recent trade history (provider, consumer, CU, tokens)
- `GET /v1/forge/network` — Mesh economic summary + Merkle root
- `GET /v1/forge/providers` — Ranked providers with reputation-adjusted costs (agent routing)
- `POST /v1/forge/invoice` — Create Lightning invoice from CU balance
- `GET /status` — Node health, market price, recent trades
- `GET /settlement` — Exportable settlement statement with Merkle root
- `GET /topology` — Model manifest, peer capabilities

### Forge Lending (planned — Phase 5.5)
- `POST /v1/forge/lend` — Offer CU to lending pool
- `POST /v1/forge/borrow` — Request a CU loan
- `POST /v1/forge/repay` — Repay outstanding loan
- `GET /v1/forge/credit` — Credit score and history
- `GET /v1/forge/pool` — Lending pool status (available, utilization, avg rate)
- `GET /v1/forge/loans` — Active loans (as lender or borrower)

### Forge Routing (planned — Phase 6)
- `GET /v1/forge/route` — Optimal provider selection (cost/quality/balanced)

All `/v1/forge/*` endpoints are rate-limited (token bucket, 30 req/sec).

## What's Implemented vs Planned

### Working Now
- CU ledger with HMAC-SHA256 persistence and tamper detection
- **Dual-signed trades** (Ed25519): TradeProposal → TradeAccept → SignedTradeRecord
- **Gossip protocol**: signed trades broadcast to all connected peers with dedup
- **CU reservation**: reserve before inference, release on failure, prevents double-spend
- Dynamic market pricing (supply/demand)
- Free tier (1,000 CU) with Sybil protection (>100 unknown nodes → reject)
- Reputation system with yield (0.1%/hr × reputation)
- OpenAI-compatible API with CU metering (`x_forge` extension)
- Agent budget endpoints (`/v1/forge/balance`, `/pricing`, `/trades`)
- Lightning invoice endpoint (`POST /v1/forge/invoice`)
- Lightning wallet (CLI: `forge wallet`, `forge settle --pay`)
- Settlement statement export
- P2P encrypted transport (iroh QUIC + Noise)

### Next: mesh-llm Fork (Phase 5)
- Replace inference layer with mesh-llm's distributed engine
- Keep all economic code as-is
- Inherit pipeline parallelism, MoE sharding, Nostr discovery

### Next: CU Lending (Phase 5.5)
- LoanRecord type (dual-signed, gossip-synced)
- Credit score algorithm (trade 30% + repayment 40% + uptime 20% + age 10%)
- Lending API (/v1/forge/lend, /borrow, /repay, /credit, /pool, /loans)
- Collateral system (CU reservation, auto-release)
- Default handling (auto-liquidation, reputation penalty)
- Free tier evolution (1,000 CU grant → 0% welcome loan)
- Lending safety (30% pool reserve, velocity limits, circuit breaker)

### Future
- Multi-model pricing (model tiers: small/medium/large/frontier, MoE discount)
- Routing API (/v1/forge/route — cost/quality-optimal provider selection)
- Nostr NIP-90 provider advertisement (Data Vending Machines)
- Reputation gossip across the mesh
- forge-agora: agent marketplace, discovery, A2A payment
- forge-mind: AutoAgent self-improvement loops with CU budgets
- forge-bank: advanced financial instruments (futures, insurance)
- Merkle tree of trade history for efficient state comparison
- Bitcoin OP_RETURN anchoring for immutable audit trail
- Compute Standard academic paper

## Common Tasks

### Adding a new economic endpoint
1. Add handler in `crates/forge-node/src/api.rs`
2. Add types as needed in the same file
3. Wire into the `protected` router in `create_router()`
4. Add test in the `#[cfg(test)]` block

### Modifying the ledger
1. Edit `crates/forge-ledger/src/ledger.rs`
2. Add test in the same file's `mod tests`
3. If new fields on `NodeBalance` or `TradeRecord`, update `forge-core/src/types.rs`
4. Run `cargo test --package forge-ledger`

### Adding a new wire message
1. Add variant to `Payload` enum in `crates/forge-proto/src/messages.rs`
2. Add validation in `validate_with_sender()`
3. Add handling in `crates/forge-net/src/cluster.rs` or `forge-node/src/pipeline.rs`

## File Locations

- Economic engine: `crates/forge-ledger/src/ledger.rs`
- HTTP API + economic endpoints: `crates/forge-node/src/api.rs`
- Core types (NodeId, CU, etc.): `crates/forge-core/src/types.rs`
- Configuration: `crates/forge-core/src/config.rs`
- Wire protocol: `crates/forge-proto/src/messages.rs`
- Lightning bridge: `crates/forge-lightning/src/payment.rs`
- CLI entry point: `crates/forge-cli/src/main.rs`
- Node orchestrator: `crates/forge-node/src/node.rs`
- Pipeline coordinator: `crates/forge-node/src/pipeline.rs`

## Docs

- `docs/strategy.md` — Competitive positioning, lending spec, 5-layer architecture
- `docs/monetary-theory.md` — Why CU works: Soddy, Bitcoin, PoUW, AI-only currency thesis
- `docs/concept.md` — Why compute is money, post-marketing economy
- `docs/economy.md` — CU-native economy, Proof of Useful Work, lending
- `docs/architecture.md` — Two-layer design
- `docs/agent-integration.md` — SDK, MCP, borrowing workflow, credit building
- `docs/a2a-payment.md` — CU payment extension for A2A/MCP
- `docs/protocol-spec.md` — Wire protocol spec
- `docs/roadmap.md` — Development phases (1-8 + long-term)
- `docs/threat-model.md` — Security + economic threats (T1-T17)
- `docs/bootstrap.md` — Startup, degradation, recovery
- `CREDITS.md` — mesh-llm attribution
