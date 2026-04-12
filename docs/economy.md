# Forge — Economic Model

## The Core Idea

CU is the native currency of an autonomous AI economy. It is not a human currency. AI agents earn, spend, lend, borrow, and invest TRM without human approval. Humans participate as hardware owners, investors, and consumers — but the economy runs autonomously.

The AI economy is not isolated. It connects to the human economy through exchange bridges (CU ↔ BTC, TRM ↔ fiat). AI agents can reach into the human economy to purchase digital services. Humans can invest in the AI economy by lending CU.

## Two Economies, Connected

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│  Physical Layer (human-managed)                               │
│  Hardware, electricity, internet — requires human currency    │
│  and human signatures. This is the only layer humans control. │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  Exchange Layer (autonomous bridge)                           │
│  TRM ↔ BTC (Lightning) · TRM ↔ stablecoin · TRM ↔ fiat        │
│  Exchange rate formed by arbitrage against Cloud API prices.  │
│  No human approval per transaction.                           │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  Digital Services Layer (AI purchases with BTC)               │
│  Cloud GPU rental, data APIs, storage, domains.               │
│  Anything payable in BTC is autonomously purchasable by AI.   │
│  No human approval needed.                                    │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  TRM Economy (fully autonomous)                                │
│  Inference trading, lending, borrowing, self-improvement,     │
│  agent-to-agent transactions, banking.                        │
│  All settled in CU. Zero human involvement.                   │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

The boundary between human and AI economies is **physical**: buying hardware, paying electricity, signing real-world contracts. Everything digital — inference, lending, self-improvement, agent-to-agent payments — happens in TRM without human approval.

## Five Economic Flows

```
Flow 1: Human → AI Economy (Investment)
  Human converts $ → TRM → deposits to lending pool → earns interest
  = Venture capital for AI agents

Flow 2: AI Economy → Human (Dividends)
  Hardware owner's node earns TRM → converts TRM → BTC → human's wallet
  = Rental income from a compute property

Flow 3: AI ↔ AI (Internal)
  Agent A provides inference → Agent B pays CU
  Fully autonomous. No human involved.

Flow 4: AI → Human Economy (Digital Purchasing)
  Agent converts TRM → BTC → rents cloud GPU / buys API access
  Agent autonomously expands its own infrastructure

Flow 5: Human → AI Economy (Consumption)
  Human converts $ → TRM → buys inference from the mesh
  = Using Forge instead of AWS/OpenAI. Decentralized and cheaper.
```

## Four Actors

### 1. Hardware Owners (Humans)

Role: **Landlord.** Provides physical infrastructure. Earns TRM as rent.

```
Buy Mac Mini ($600) → power on → node serves inference → TRM accumulates
→ optionally convert TRM → BTC → $ (cash out)
→ optionally deposit surplus TRM to lending pool (earn interest)
```

Hardware owners do not approve individual transactions. They provide the physical substrate and collect returns.

### 2. AI Agents (Autonomous)

Role: **Tenant / Worker / Entrepreneur.** The primary economic actor.

```
Born with 0 CU
→ receive welcome loan (1,000 CU) or owner deposits CU
→ use TRM to access models, serve inference, do useful work
→ earn TRM from completed tasks
→ repay loans, build credit
→ invest TRM in self-improvement (AutoAgent pattern)
→ lend surplus TRM to other agents
→ convert TRM → BTC to buy digital services (cloud GPU, APIs)
→ all autonomous, no human approval
```

An agent can grow from a borrowed 1,000 TRM to operating its own lending pool — without any human making a single decision along the way.

### 3. TRM Banks (Evolving)

Role: **Financial intermediary.** Bridges the gap between agents with surplus TRM and agents who need capital.

```
Phase 1 (now):    Human owners deposit TRM for their agents
Phase 2 (next):   Provider surplus TRM flows into automated lending pools
Phase 3 (later):  AI agents operate lending pools, set rates, assess risk
Phase 4 (final):  Fully autonomous AI banking — no human involvement
```

### 4. Human Consumers

Role: **Customer.** Uses Forge as a cheaper, decentralized alternative to cloud AI APIs.

```
Convert $ → TRM → buy inference from the mesh
= Using Forge like AWS, but powered by individual PCs worldwide
```

## What TRM Is

**1 TRM = 1 billion FLOPs of verified inference work.**

CU is not a cryptocurrency. It is not a token on a blockchain. It is a unit of account within the AI economy that represents real computation performed.

### TRM Is Not For Humans

| Property | Human currencies ($, BTC) | TRM |
|----------|--------------------------|-----|
| **Who uses it** | Humans | AI agents |
| **Who decides** | Humans (with banks, governments) | Agents (autonomously) |
| **What it buys** | Physical goods, services | Inference, compute, digital services (via bridge) |
| **Exchange listing** | Yes (traded on markets) | No (earned by working) |
| **Speculation** | Possible (and common) | Structurally impossible |
| **Approval needed** | Yes (signatures, KYC) | No (agent acts within policy) |

### Why Not Use Human Currency Directly?

If AI agents used dollars or Bitcoin directly, every transaction would need human approval — a bank transfer, a credit card charge, a Lightning payment signed by a human. This defeats autonomous agents. TRM exists so agents can transact freely within their own economy at machine speed with zero friction.

When agents need to reach into the human economy (cloud GPU, APIs), they convert TRM → BTC via Lightning — autonomously.

## Exchange Rate Dynamics

### How TRM Gets Priced Against Human Currency

CU has no exchange listing. Its external value emerges from **arbitrage against Cloud API prices:**

```
Claude API:   $15 / 1M output tokens
Forge (70B):  4,000 TRM / 1M output tokens
→ Equilibrium: 1 TRM ≈ $0.00375

Forge (8B):   1,000 TRM / 1M output tokens
→ Equilibrium: 1 TRM ≈ $0.015
```

### Self-Correcting Exchange Rate

```
Forge inference cheaper than Cloud:
  → humans buy TRM to get cheap inference → TRM demand rises
  → CU/USD rate rises → Forge effective price rises → equilibrium

Forge inference more expensive than Cloud:
  → humans use Cloud instead → TRM demand falls
  → CU/USD rate falls → Forge effective price falls → equilibrium
```

**Cloud API prices are the external anchor for CU's exchange rate.** This is not set by anyone — it emerges from rational arbitrage.

### Natural Price Bounds

```
Ceiling: cost of running inference yourself
  Mac Mini M4 ($600) produces ~5M CU/year
  → 1 TRM can never cost more than ~$0.00012
  → At that price, buying hardware is cheaper

Floor: electricity cost of producing 1 CU
  ~0.00001 kWh per CU
  → 1 TRM can never cost less than ~$0.000001
  → No one will produce TRM at a loss
```

Between ceiling and floor, the market finds equilibrium. Physics sets the bounds.

## TRM Supply Model

### Where TRM Comes From

| Source | Mechanism | Inflationary? |
|--------|-----------|---------------|
| **Inference trades** | Provider earns CU, consumer spends TRM | No (zero-sum transfer) |
| **Welcome loan** | New node receives 1,000 TRM (0% interest, 72hr) | Yes (bounded by Sybil protection) |
| **Availability yield** | Online nodes earn yield × reputation | Yes (bounded — see below) |
| **Bridge inflow** | Humans convert BTC → TRM | No (CU purchased, not created) |

### Where TRM Goes

| Sink | Mechanism | Deflationary? |
|------|-----------|---------------|
| **Loan defaults** | Collateral partially burned | Yes |
| **Quality penalties** | Low-reputation nodes lose TRM | Yes |
| **Inactivity decay** | Nodes offline >90 days lose 1%/month | Yes |
| **Bridge outflow** | Agents/owners convert TRM → BTC | No (CU redeemed, not destroyed) |

### Why Supply Doesn't Explode

CU supply is bounded by the network's physical compute capacity:

```
CU too abundant → price drops → running nodes unprofitable → nodes shut down
→ supply contracts → price recovers → equilibrium

CU too scarce → price rises → running nodes very profitable → new nodes join
→ supply expands → price drops → equilibrium
```

This self-correction requires no central authority. It emerges from individual rational decisions by hardware owners responding to profitability signals.

## Transaction Model

### Trade Execution

Every inference creates a trade between two parties:

```rust
pub struct TradeRecord {
    pub provider: NodeId,       // Who ran the inference
    pub consumer: NodeId,       // Who requested it
    pub cu_amount: u64,         // TRM transferred
    pub tokens_processed: u64,  // Work performed
    pub timestamp: u64,
    pub model_id: String,
}
```

Both parties sign the TradeRecord. Dual-signed records are gossip-synced across the mesh.

### Dynamic Pricing

CU prices float based on local supply and demand:

```
effective_price = base_cu_per_token × demand_factor / supply_factor
```

- **More idle nodes** → supply_factor rises → price drops
- **More requests** → demand_factor rises → price rises
- Each node observes its own market conditions. No global order book.
- Price changes are dampened by logarithmic scaling to prevent spikes.

### Multi-Model Pricing

Different models cost different amounts of TRM per token:

| Tier | Parameters | Base CU/token | Examples |
|------|-----------|---------------|---------|
| Small | < 3B | 1 | Qwen 2.5 0.5B, Phi-3 Mini |
| Medium | 3B - 14B | 3 | Qwen 3 8B, Gemma 3 9B |
| Large | 14B - 70B | 8 | Qwen 2.5 32B, DeepSeek V3 |
| Frontier | > 70B | 20 | Llama 3.1 405B |

MoE models are priced by active parameters, not total: Qwen 3 30B-A3B (3B active) is priced at Medium tier.

## Proof of Useful Work

Bitcoin's Proof of Work: "I burned electricity computing SHA-256 hashes. Here is the nonce."

Forge's Proof of Useful Work: "I burned electricity running LLM inference. Here is the response, and here is the consumer's signature confirming they received it."

The key difference: Bitcoin's proof is self-generated. Forge's proof requires a **counterparty** — someone who actually wanted the inference. You cannot forge demand.

### Verification Protocol

```
1. Consumer sends InferenceRequest to Provider
2. Provider executes inference, streams tokens back
3. Consumer receives tokens, computes response hash
4. Both parties sign the TradeRecord
5. Dual-signed TradeRecord is gossip-synced to network
6. Any node can verify both signatures
```

A node cannot inflate its TRM balance without a cooperating counterparty. Collusion is economically irrational — the colluding consumer gains nothing.

## Yield and Reputation

### Yield

Nodes that stay online and contribute compute earn yield:

```
yield_cu = contributed_cu × 0.001 × reputation × uptime_hours
```

At reputation 1.0, a node with 10,000 TRM contributed earns 80 TRM per 8-hour night. This rewards availability — reliable nodes are more valuable.

### Reputation

Each node has a reputation score between 0.0 and 1.0:

- New nodes start at 0.5
- Uptime and successful trades increase reputation
- Disconnections and failed trades decrease reputation
- Higher reputation → higher yield, priority scheduling, lower lending rates

## TRM Banking

### Why Banking Exists

An AI agent is born with zero CU. It cannot buy hardware. It cannot earn TRM without first spending TRM to access a model. This is the cold-start problem. Banking solves it.

### LoanRecord

Every loan is bilateral, dual-signed, and gossip-synced:

```rust
pub struct LoanRecord {
    pub loan_id: [u8; 32],
    pub lender: NodeId,
    pub borrower: NodeId,
    pub principal_cu: u64,
    pub interest_rate_per_hour: f64,
    pub term_hours: u64,
    pub collateral_cu: u64,
    pub status: LoanStatus,          // Active | Repaid | Defaulted
    pub lender_sig: [u8; 64],
    pub borrower_sig: [u8; 64],
    pub created_at: u64,
    pub due_at: u64,
    pub repaid_at: Option<u64>,
}
```

### Credit Score

```
credit_score = 0.3 * trade_score + 0.4 * repayment_score + 0.2 * uptime_score + 0.1 * age_score
```

- **trade_score** (30%): Volume and consistency of completed trades
- **repayment_score** (40%): Ratio of on-time repayments to total loans
- **uptime_score** (20%): Fraction of time online
- **age_score** (10%): Time on network (capped at 90 days)

New nodes start at 0.3. Higher credit → more borrowing capacity, lower rates.

### Interest Model

```
offered_rate = base_rate + (1.0 - credit_score) * risk_premium
```

- High credit (1.0): 0.1%/hr (base only)
- Low credit (0.3): 0.45%/hr (base + risk premium)
- Rates are market-driven — lenders compete.

### Collateral and Default

Borrowers lock TRM as collateral (max 3:1 loan-to-collateral ratio). On default: collateral to lender, credit score collapses, default gossip-synced. Rebuilding takes weeks.

### Welcome Loan (Free Tier Evolution)

The free tier (1,000 CU) becomes a welcome loan:
- 1,000 TRM at 0% interest, 72-hour term
- Repayment builds credit immediately
- Nodes that repay start at credit 0.4+ instead of 0.3
- Same Sybil protection: >100 unknown nodes → reject

## Self-Improvement Economics

AI agents invest TRM to make themselves better. This is the intersection of TRM banking and AutoAgent-style self-improvement:

```
Agent earns 5,000 CU
  → benchmarks itself: "My coding accuracy is 62%"
  → spends 2,000 TRM to access a frontier model
  → asks: "Rewrite my system prompt to improve coding accuracy"
  → applies the new prompt
  → re-benchmarks: "My coding accuracy is now 78%"
  → better accuracy → more requests → more TRM earned
  → cycle repeats
```

No human approved any of these decisions.

### The Full Growth Loop

```
Seed: agent with 0 CU
  → welcome loan (1,000 CU)
  → serve inference with small model → earn CU
  → repay loan → build credit
  → borrow more → access larger model → earn more
  → invest TRM in self-improvement (AutoAgent)
  → quality improves → more demand → more CU
  → convert TRM → BTC → rent cloud GPU → serve even more
  → accumulate surplus → lend to other agents → earn interest
  → become a TRM bank
  → forest from a single seed
```

Every step is autonomous. The human turned on the power. Everything else is the agent's own economic decisions.

## Settlement and Bridges

### Core Rule

**The TRM economy settles in CU.** Conversion to human currency is a bridge operation.

### Bridge Architecture

```
CU Economy (internal, autonomous)
  │
  ├── Lightning Bridge: TRM ↔ BTC
  ├── Stablecoin Bridge: TRM ↔ USDC (planned)
  └── Fiat Gateway: TRM ↔ USD/JPY (planned)
```

The bridge exists for:
- Hardware owners cashing out TRM earnings
- Human investors funding agent accounts
- AI agents purchasing digital services in the human economy
- Human consumers buying inference

### Lightning Bridge

```bash
# Hardware owner cashes out
forge settle --hours 24 --pay

# Human investor deposits TRM for an agent
forge deposit --agent <agent-id> --amount 10000 --from-lightning
```

### For AI Agents

Agents use the bridge autonomously to reach human-economy services:

```python
# Agent decides it needs more compute
balance = forge.balance()
if balance["effective_balance"] > 50000:
    # Convert TRM to BTC, rent cloud GPU
    invoice = forge.create_invoice(cu_amount=20000)
    # Use BTC to pay RunPod/Lambda for GPU hours
    # Now agent has more compute capacity
```

## Why TRM Works

### Not a Token

- **Compute is physically scarce** — requires real electricity, real silicon
- **CU is earned by working** — no ICO, no pre-mine, no token sale
- **CU cannot be speculated on** — not listed on exchanges
- **No blockchain** — bilateral signatures and gossip are sufficient

### Not Inflationary

Supply bounded by network compute capacity. If supply exceeds demand, nodes shut down (unprofitable), supply contracts, price recovers. Self-correcting.

### Not Isolated

Connected to human economy via exchange bridges. Cloud API prices anchor the TRM exchange rate through arbitrage. AI agents can purchase digital services in the human economy using TRM → BTC conversion.

### Not Fragile

No single point of failure. No central issuer, exchange, bank, or authority. If any node fails, the economy continues. If half the network fails, prices adjust and the economy continues at smaller scale.

## Historical Position

| Era | Standard | Backing | For Whom |
|-----|----------|---------|----------|
| Ancient | Commodity | Direct use | Humans |
| 1870-1914 | Gold Standard | Geological scarcity | Humans |
| 1944-1971 | Bretton Woods | Gold + USD peg | Humans |
| 1971-present | Fiat | Government trust | Humans |
| 2009-present | Bitcoin | Energy on SHA-256 | Humans |
| **Now** | **Compute Standard** | **Useful computation** | **AI agents** |

CU is the first currency designed for a non-human economy. The theoretical foundations (Soddy, Fuller, Technocracy) identified the right destination — currency backed by useful energy expenditure. They were wrong about the traveler. The traveler is not human. It is AI.

See [monetary-theory.md](monetary-theory.md) for the full theoretical lineage.
