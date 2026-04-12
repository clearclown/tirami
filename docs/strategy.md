# Forge — Strategy

## Competitive Landscape

| Project | Market Cap | Approach | Token | Key Weakness |
|---------|-----------|----------|-------|-------------|
| **Bittensor** | $2.9B | Subnet-based AI marketplace, Yuma Consensus | TAO (speculative) | Validator gaming, quality measurement unsolved |
| **Akash** | $118M | General cloud compute, reverse auction | AKT (burn-mint) | Not AI-native, no agent economy |
| **Autonolas** | $10.5M | Autonomous agent economy, Mech Marketplace | OLAS (fee-burn) | Token-dependent, no self-improvement |
| **Render** | $976M | GPU rendering + AI workloads | RENDER | Rendering-first, AI is recent pivot |
| **io.net** | $30M | GPU aggregation (30K+ GPUs claimed) | IO | Low utilization, centralization concerns |
| **Gensyn** | Pre-token | Trustless ML training verification | Planned | Training-only, no inference |
| **Ritual** | Pre-token | On-chain AI inference (8K+ nodes) | Planned | Latency/cost of on-chain inference |
| **Golem** | $125M | General compute marketplace (est. 2016) | GLM | No GPU-first design, no AI features |
| **Morpheus** | $9.6M | AI agent network with wallets | MOR | Very early, basic agent capabilities |

**Industry-wide weakness:** Most networks have excess supply and insufficient demand. io.net claims 30,000+ GPUs but has a $30M market cap. Golem has run since 2016 with minimal commercial adoption. The demand side is the unsolved problem.

**What every competitor has in common:** A speculative token as the settlement layer. None use compute itself as currency.

## Forge's Three Differentiators

### 1. CU-Native Economics (No Speculative Token)

Every competitor settles in a tradeable token (TAO, AKT, GLM, RENDER, IO). Token value is driven by speculation, not utility. When token prices crash, provider incentives evaporate.

Forge settles in TRM — a unit backed by verified useful computation. TRM cannot be pre-mined, ICO'd, or speculated on. Its value is intrinsic: 1 TRM represents real inference work that someone actually needed.

### 2. Compute Lending With Interest

**No other distributed inference project offers this.** This was confirmed through comprehensive competitive analysis.

Existing projects: you either have compute hardware or you don't participate. Forge enables nodes without sufficient resources to borrow CU, access larger models, earn from better inference, and repay with interest. This is the key to lowering the participation barrier and solving the demand-side problem.

### 3. Agent-First Budget Management

No major AI agent framework (AutoGPT, CrewAI, LangGraph, LangChain) has a built-in economic layer. Agents cannot autonomously manage compute budgets, borrow resources, or make cost/quality tradeoffs.

Forge's `/v1/tirami/balance`, `/pricing`, `/credit`, and `/borrow` endpoints let agents operate as fully autonomous economic actors within human-set policy limits.

## 5-Layer Architecture

```
┌─────────────────────────────────────────────────┐
│  Layer 4: Discovery (tirami-agora)               │
│  Agent marketplace, reputation aggregation,     │
│  Nostr NIP-90, Google A2A payment extension     │
├─────────────────────────────────────────────────┤
│  Layer 3: Intelligence (tirami-mind)             │
│  AutoAgent self-improvement loops,              │
│  harness marketplace, meta-optimization         │
├─────────────────────────────────────────────────┤
│  Layer 2: Finance (tirami-bank)                  │
│  TRM lending, yield optimization, credit,        │
│  futures, insurance, derivatives                │
├─────────────────────────────────────────────────┤
│  Layer 1: Economy (forge — this repo)           │
│  TRM ledger, dual-signed trades, dynamic pricing,│
│  lending primitives, safety controls            │
├─────────────────────────────────────────────────┤
│  Layer 0: Inference (forge-mesh / mesh-llm)     │
│  Pipeline parallelism, MoE sharding,            │
│  iroh mesh, Nostr discovery, MLX/llama.cpp      │
└─────────────────────────────────────────────────┘
```

**Separation principle:** Layers 0-1 are the protocol core (this repo + forge-mesh). Layer 2 lending primitives live in tirami-ledger (protocol-level). Advanced Layer 2 instruments and Layers 3-4 are separate repositories built on top of the protocol.

## Repository Ecosystem

| Repository | Language | Status | Layer | Purpose |
|-----------|----------|--------|-------|---------|
| **forge** | Rust | Active | L1 | Protocol core: TRM ledger, trades, lending primitives, safety |
| **forge-mesh** | Rust | Active | L0 | mesh-llm + Forge economic layer = production runtime |
| **tirami-sdk** | Python | Published (PyPI) | Client | Python SDK for Forge API |
| **forge-cu-mcp** | Python | Published (PyPI) | Client | MCP server for AI tools (Claude, ChatGPT, Cursor) |
| **tirami-bank** | Rust + Python | Planned | L2 | Advanced financial instruments (futures, insurance) |
| **tirami-mind** | Python | Planned | L3 | AutoAgent self-improvement + TRM economy |
| **tirami-agora** | Python/TypeScript | Planned | L4 | Agent marketplace, Nostr NIP-90, A2A |

**Naming rationale:**
- **forge** — The foundry. Where value is created from raw compute.
- **forge-mesh** — The network mesh. Physical inference execution.
- **tirami-bank** — Financial services layer.
- **tirami-mind** — Intelligence. Self-improving agents.
- **tirami-agora** — Ancient Greek marketplace. No advertising, pure merit-based trade.

## TRM Lending Specification

### Problem

To participate in Forge, you need hardware capable of running LLM inference. This creates a participation barrier identical to Bitcoin's ASIC problem. Lending solves this by enabling economic participation without upfront hardware investment.

### Participation Paths (enabled by lending)

| Path | Requirements | How it works |
|------|-------------|-------------|
| **A: Hardware owner** | PC/Mac/GPU | Run node → earn TRM → optionally lend surplus |
| **B: Weak hardware** | Old PC or phone | Borrow TRM → access large models → earn → repay |
| **C: Skills only** | No hardware | Contribute harnesses/curation → earn TRM → participate |
| **D: Capital only** | Money, no hardware | Buy TRM via Lightning → lend to pool → earn yield |

Path B is critical — without it, Forge cannot achieve network effects.

### LoanRecord Structure

```rust
pub struct LoanRecord {
    pub loan_id: [u8; 32],           // Unique identifier (hash of terms)
    pub lender: NodeId,
    pub borrower: NodeId,
    pub principal_cu: u64,           // Amount lent
    pub interest_rate_per_hour: f64, // e.g., 0.005 = 0.5%/hr
    pub term_hours: u64,             // Loan duration
    pub collateral_cu: u64,          // Borrower's TRM locked as collateral
    pub status: LoanStatus,          // Active | Repaid | Defaulted
    pub lender_sig: [u8; 64],        // Ed25519 signature
    pub borrower_sig: [u8; 64],      // Ed25519 signature
    pub created_at: u64,             // Timestamp
    pub due_at: u64,                 // created_at + term_hours * 3600
    pub repaid_at: Option<u64>,      // Timestamp of repayment (if repaid)
}

pub enum LoanStatus {
    Active,
    Repaid,
    Defaulted,
}
```

LoanRecords are dual-signed and gossip-synced, identical to TradeRecords.

### Credit Score Algorithm

```
credit_score = 0.3 * trade_score + 0.4 * repayment_score + 0.2 * uptime_score + 0.1 * age_score
```

| Component | Weight | Calculation | Range |
|-----------|--------|-------------|-------|
| `trade_score` | 30% | `min(1.0, total_trade_volume / 100_000)` | 0.0-1.0 |
| `repayment_score` | 40% | `on_time_repayments / total_loans` (0 if no loans) | 0.0-1.0 |
| `uptime_score` | 20% | `hours_online / hours_since_join` | 0.0-1.0 |
| `age_score` | 10% | `min(1.0, days_on_network / 90)` | 0.0-1.0 |

**Cold start:** New nodes start at 0.3 (below the 0.5 default reputation). Nodes with no loan history use `repayment_score = 0.5` (neutral).

**Score decay:** If a node is inactive for >7 days, uptime_score decays at 0.01/day.

### Maximum Borrowable Amount

```
max_borrow = credit_score * credit_score * pool_available * 0.2
```

At credit 0.3: max 1.8% of pool. At credit 0.7: max 9.8% of pool. At credit 1.0: max 20% of pool.

The quadratic relationship rewards sustained good behavior disproportionately.

### Interest Rate Model

```
offered_rate = base_rate + (1.0 - credit_score) * risk_premium
```

- `base_rate`: 0.1% per hour (set by lender, market-driven)
- `risk_premium`: 0.5% per hour maximum
- At credit 1.0: rate = 0.1%/hr (base only)
- At credit 0.3: rate = 0.45%/hr (base + 70% of premium)

### Lending API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `POST /v1/tirami/lend` | POST | Offer TRM to lending pool. Params: `amount`, `max_term_hours`, `min_interest_rate` |
| `POST /v1/tirami/borrow` | POST | Request TRM loan. Params: `amount`, `term_hours`, `collateral` |
| `POST /v1/tirami/repay` | POST | Repay outstanding loan. Params: `loan_id`, `amount` |
| `GET /v1/tirami/credit` | GET | View credit score, components, and history |
| `GET /v1/tirami/pool` | GET | View lending pool status (available, total lent, utilization, avg rate) |
| `GET /v1/tirami/loans` | GET | View active loans (as lender or borrower) |

### Safety Guardrails

| Guardrail | Value | Purpose |
|-----------|-------|---------|
| Max loan-to-collateral ratio | 3:1 | Limit exposure per loan |
| Max single loan (% of pool) | 20% | Prevent pool concentration |
| Pool reserve requirement | 30% | Always keep 30% of pool unlent |
| Lending velocity limit | 10 new loans/min | Prevent rapid drain attacks |
| Default rate circuit breaker | >10% defaults/hour → suspend lending | Prevent cascading defaults |
| Min credit for borrowing | 0.2 | Reject untrusted nodes |
| Max term | 168 hours (7 days) | Limit long-term exposure |

### Free Tier Evolution

The current free tier (1,000 TRM grant) evolves into the first loan:

| Current | Target |
|---------|--------|
| 1,000 TRM granted free | 1,000 TRM lent at 0% interest, 72-hour term |
| No repayment obligation | Repayment builds credit score |
| Sybil check: >100 unknown nodes → reject | Same, plus credit_score < 0.2 → reject |

Nodes that repay the "welcome loan" start building credit immediately. Nodes that don't repay get credit_score = 0.0 and cannot borrow again.

## Multi-Model Pricing Specification

### Model Tiers

| Tier | Parameters | Base CU/token | Examples |
|------|-----------|---------------|---------|
| Small | < 3B | 1 | Qwen 2.5 0.5B, Phi-3 Mini |
| Medium | 3B - 14B | 3 | Qwen 3 8B, Gemma 3 9B, Llama 3.2 8B |
| Large | 14B - 70B | 8 | Qwen 2.5 32B, DeepSeek V3 67B |
| Frontier | > 70B | 20 | Llama 3.1 405B, DeepSeek R1 |

### MoE Discount

Mixture-of-Experts models activate only a fraction of total parameters per token. Pricing reflects actual compute:

```
moe_price = base_price * (active_params / total_params)
```

Example: Qwen 3 30B-A3B (30B total, 3B active) → priced at Medium tier (3B active), not Large.

### Dynamic Adjustment

Model tier pricing is a base. Actual price still floats with supply/demand:

```
actual_price = tier_base * demand_factor / supply_factor
```

## Routing API

### `GET /v1/tirami/route`

Returns the optimal provider for a given request.

**Parameters:**
- `model`: Required model or minimum capability
- `max_cu`: Maximum TRM budget for this request
- `mode`: `cost` | `quality` | `balanced` (default: `balanced`)
- `max_tokens`: Expected output length

**Response:**
```json
{
  "provider": "<node-id>",
  "model": "qwen3-8b-q4",
  "estimated_cu": 24,
  "provider_reputation": 0.87,
  "estimated_latency_ms": 450,
  "score": 0.82
}
```

**Scoring algorithm:**
```
score = reputation * quality_weight - normalized_price * cost_weight + latency_bonus

where:
  cost mode:     quality_weight = 0.3, cost_weight = 0.7
  quality mode:  quality_weight = 0.7, cost_weight = 0.3
  balanced mode: quality_weight = 0.5, cost_weight = 0.5
```

## Nostr NIP-90 Compatibility

mesh-llm already uses Nostr for peer discovery. NIP-90 ("Data Vending Machines") extends this naturally:

| NIP-90 Concept | Forge Mapping |
|---------------|---------------|
| Job request (kind 5050) | Inference request with TRM budget |
| Service provider | Forge node serving inference |
| Job result (kind 6050) | Inference response with TRM cost |
| Payment (Lightning zap) | TRM transfer (or Lightning via bridge) |
| Provider discovery | Nostr relay + Agent Card |

### Integration approach

1. Forge providers publish NIP-90 `kind:31990` handler events advertising models and TRM pricing
2. Consumers discover providers via Nostr relays
3. Job requests include `X-Forge-Max-CU` tag
4. Responses include `X-Forge-CU-Cost` tag
5. Settlement happens via TRM protocol (bilateral signed trade) or Lightning (NIP-57 zap)

This gives Forge instant access to Nostr's existing relay infrastructure without building a separate discovery network.

## Phased Roadmap

| Phase | Name | Status | Key Deliverables |
|-------|------|--------|-----------------|
| 1 | Local Inference | Done | llama.cpp, GGUF, streaming, CLI |
| 2 | P2P Protocol | Done | iroh QUIC, Noise, 14 message types |
| 3 | Operator Ledger | Done | TRM accounting, HMAC-SHA256 integrity |
| 4 | Economic API | Done | OpenAI-compatible API, TRM metering, agent endpoints |
| **5** | **mesh-llm Fork** | **Next** | Replace inference layer, inherit pipeline parallelism, MoE, Nostr |
| **5.5** | **CU Lending** | **Planned** | LoanRecord, credit score, lending API, collateral, safety |
| **6** | **Multi-Model Pricing** | **Planned** | Model tiers, MoE discount, routing API |
| **7** | **Discovery + Marketplace** | **Planned** | Reputation gossip, NIP-90, tirami-agora |
| **8** | **Agent Intelligence** | **Planned** | tirami-mind, AutoAgent loops, self-reinforcement |

See [roadmap.md](roadmap.md) for detailed deliverables per phase.

## What Forge Is NOT

1. **Not a speculative token.** TRM is earned by performing useful work, not purchased or traded on exchanges. There is no ICO, no governance token, no token sale.

2. **Not a blockchain.** TRM accounting uses local ledgers + gossip + dual signatures. No global consensus mechanism. No smart contracts. Bitcoin anchoring is optional.

3. **Not a centralized authority.** No single entity controls TRM issuance, pricing, or access. Market prices emerge from local supply and demand observations.

4. **Not a general compute platform.** Forge is optimized for LLM inference, not arbitrary computation. This focus enables inference-specific optimizations (model-aware pricing, MoE discounts, quality verification).

5. **Not dependent on Bitcoin.** Lightning is an optional off-ramp for operators who need external liquidity. The protocol functions with zero external currency.

## Local AI Context

The economic viability of running a Forge node on consumer hardware is crossing a threshold:

| Hardware | Cost | Model | Speed | Monthly electricity |
|----------|------|-------|-------|-------------------|
| Mac Mini M4 16GB | $600 | Qwen 3 8B Q4 | ~32 tok/s | ~$5 |
| Mac Mini M4 Pro 64GB | $2,000 | Qwen 3 30B-A3B (MoE) | ~130 tok/s | ~$8 |
| Mac Mini M4 Max 128GB | $4,000 | DeepSeek-R1 70B Q4 | ~12 tok/s | ~$10 |

**Key trends:**
- Ollama 0.19 switched to Apple MLX backend: 1.6x prefill, 2x decode speed improvement
- MoE models (Qwen 3 30B-A3B) run at 100+ tok/s on Apple Silicon — only 3B active parameters per token
- M5 Max (expected mid-2026) projects ~4x faster TTFT, ~12% faster decode
- Cloud API costs ($5-15/M tokens) vs local inference ($5-10/month electricity) — local wins within months

**Implication:** A $600 Mac Mini running 24/7 as a Forge node is economically viable as a "compute rental property" — generating TRM yield while the owner sleeps. TRM lending makes this accessible even to nodes that can't afford upfront hardware investment.
