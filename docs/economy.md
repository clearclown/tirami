# Forge — Economic Model

## Compute Standard (計算本位制)

Every monetary system is backed by scarcity:

| Era | Standard | Backing |
|-----|----------|---------|
| Ancient | Gold/Silver | Geological scarcity |
| 1944–1971 | Bretton Woods | USD pegged to gold |
| 1971–present | Petrodollar | Oil demand + military power |
| 2009–present | Bitcoin | Electricity burned on SHA-256 |
| **Forge** | **Compute Standard** | **Electricity spent on useful inference** |

Forge introduces a Compute Standard: the unit of value is backed by real energy expenditure performing useful computation. Unlike Bitcoin's Proof of Work, every joule spent in Forge produces real intelligence.

## CU: The Native Currency

### What CU Is

**1 CU = 1 billion FLOPs of verified inference work.**

CU is not a cryptocurrency. It is not a token on a blockchain. It is a unit of account that represents real computation performed. CU has value because it is a claim on future compute — if you earned CU by serving inference, you can spend it to receive inference.

### Why CU, Not Bitcoin

| Property | CU | Bitcoin |
|----------|-----|---------|
| **Value backing** | Useful computation (intrinsic) | Hash computation (artificial) |
| **Settlement speed** | Instant (local ledger) | Seconds to minutes (Lightning/chain) |
| **Transaction cost** | Zero | Channel fees, on-chain fees |
| **External dependency** | None | Bitcoin network health |
| **Quantum risk** | None (no cryptographic puzzle) | SHA-256 / ECDSA vulnerable |
| **Yield generation** | Yes (idle hardware earns CU) | No (BTC in wallet earns nothing) |

CU is the **primary** settlement unit. Bitcoin, stablecoins, and fiat are optional **off-ramps** available through bridge adapters outside the protocol.

### CU as a Productive Asset

```
Apartment building          Mac Mini on Forge
───────────────────         ──────────────────
Asset: building             Asset: compute hardware
Cost: maintenance           Cost: electricity
Revenue: rent               Revenue: CU from inference
Yield: rent - maintenance   Yield: CU earned - electricity cost
Idle = lost income          Idle = wasted potential
```

A computing device on Forge is not like Bitcoin in a wallet (static value, no yield). It is like a rental property — generating income through useful work.

## Transaction Model

### Trade Execution

Every inference creates a trade between two parties:

```rust
pub struct TradeRecord {
    pub provider: NodeId,       // Who ran the inference
    pub consumer: NodeId,       // Who requested it
    pub cu_amount: u64,         // CU transferred
    pub tokens_processed: u64,  // Work performed
    pub timestamp: u64,
    pub model_id: String,
}
```

The trade is recorded by both parties. In the current implementation, each node maintains a local ledger. The target implementation adds dual signatures and gossip sync.

### Dynamic Pricing

CU prices float based on local supply and demand:

```
effective_price = base_cu_per_token × demand_factor / supply_factor
```

- **More idle nodes** → supply_factor rises → price drops
- **More inference requests** → demand_factor rises → price rises
- Each node observes its own market conditions. No global order book.

### Free Tier

New nodes with no contribution history receive 1,000 CU. This lets anyone use the network immediately. The free tier is consumed from the first request — it does not reset.

Sybil mitigation: if more than 100 unknown nodes have appeared without contributing, new free-tier requests are rejected.

## Proof of Useful Work

### The Concept

Bitcoin's Proof of Work: "I burned electricity computing SHA-256 hashes. Here is the nonce that proves it."

Forge's Proof of Useful Work: "I burned electricity running LLM inference. Here is the response, and here is the consumer's signature confirming they received it."

The key difference: Bitcoin's proof is self-generated (any miner can produce a valid hash). Forge's proof requires a **counterparty** — someone who actually wanted the inference. You cannot forge demand.

### Verification Protocol (target)

```
1. Consumer sends InferenceRequest to Provider
2. Provider executes inference, streams tokens back
3. Consumer receives tokens, computes response hash
4. Both parties sign the TradeRecord:
   - Provider signs: "I computed this"
   - Consumer signs: "I received this"
5. Dual-signed TradeRecord is gossip-synced to network
6. Any node can verify both signatures
```

A node cannot inflate its CU balance without a cooperating counterparty. Collusion is possible but economically irrational — the colluding consumer gains nothing by signing fake trades.

### Current Implementation

The current reference implementation uses local ledgers with HMAC-SHA256 integrity protection. Dual signatures and gossip are the next step.

## Yield and Reputation

### Yield

Nodes that stay online and contribute compute earn yield:

```
yield_cu = contributed_cu × 0.001 × reputation × uptime_hours
```

At reputation 1.0, a node with 10,000 CU contributed earns 80 CU per 8-hour night. This is not inflation — it is a reward for availability. Nodes that are reliably online are more valuable to the network.

### Reputation

Each node has a reputation score between 0.0 and 1.0:

- New nodes start at 0.5
- Uptime and successful trades increase reputation
- Disconnections and failed trades decrease reputation
- Higher reputation → higher yield rate, priority in scheduling

## Settlement and External Bridges

### Core Rule

**The protocol settles in CU.** Conversion to anything else is an integration concern.

### Settlement Statements

Operators can export auditable trade histories for any time window:

```
forge settle --hours 24 --price 0.05 --out settlement.json
```

The statement includes: gross CU earned, gross CU spent, net CU, trade count, and optional reference price per CU.

### Bridge Architecture

```
Layer 0: Forge protocol
  → CU accounting, trades, pricing

Layer 1: Settlement statement
  → Exportable trade history
  → Reference exchange rate

Layer 2: External bridge (optional)
  → CU ↔ BTC (Lightning)
  → CU ↔ stablecoin
  → CU ↔ fiat
```

The bridge layer is outside the protocol. Different operators can use different bridges. The protocol remains useful with zero external liquidity.

### Lightning Bridge

For operators who want Bitcoin settlement:

```bash
forge settle --hours 24 --pay
```

This creates a BOLT11 Lightning invoice for the net CU earned, converted at the configured exchange rate (default: 10 msats per CU).

## Agent-Directed Budgets

### The Vision

Traditional: Human decides → Human pays → AI executes
Forge: Policy allows agent → Agent checks budget → Agent spends CU → Agent executes

### API

```
GET /v1/forge/balance   → CU balance, contribution, consumption, reputation
GET /v1/forge/pricing   → Market price, cost estimates per 100/1000 tokens
```

An agent can:
1. Check its balance before making a request
2. Estimate the cost of inference at current market prices
3. Decide whether the request is worth the CU cost
4. Execute and pay automatically

Human supervisors set budget policies. Agents operate within those limits autonomously.

### Self-Reinforcement Loop

```
Agent (small, phone)
  → earns CU by lending idle compute
  → spends CU on larger model access
  → becomes smarter
  → makes better economic decisions
  → earns more CU
  → accesses even larger models
  → ...
```

This is a possible application pattern. The protocol provides the market; agents provide the strategy.

## CU Lending

### Compute Microfinance

Traditional finance has microloans: a farmer borrows to buy seeds, grows crops, repays with interest. Forge has micro-compute-loans: a node borrows CU to access a larger model, serves premium inference, repays from earnings.

```
Apartment renovation loan         CU loan on Forge
───────────────────────           ──────────────────
Borrow: $50,000                   Borrow: 5,000 CU
Use: renovate units               Use: access 70B model
Revenue: higher rent              Revenue: premium inference fees
Repay: loan + interest            Repay: CU + interest
Result: net profit                Result: net CU profit
```

Without lending, small nodes are permanently stuck at the small-model tier. With lending, any node can temporarily access frontier-class models and earn its way up.

### LoanRecord

Every loan is a bilateral agreement between lender and borrower, dual-signed like TradeRecords:

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

LoanRecords are gossip-synced across the mesh. Any node can verify both signatures and track the lending state of the network.

### Credit Score

Each node computes credit scores locally from observed behavior:

```
credit_score = 0.3 * trade_score + 0.4 * repayment_score + 0.2 * uptime_score + 0.1 * age_score
```

- **trade_score** (30%): Volume and consistency of completed inference trades
- **repayment_score** (40%): Ratio of on-time repayments to total loans taken
- **uptime_score** (20%): Fraction of time online since joining
- **age_score** (10%): Time on the network (capped at 90 days for full score)

New nodes start at credit score 0.3. Nodes with no loan history use a neutral repayment_score of 0.5.

### Lending Pool

Individual lenders contribute CU to a node-local lending pool:

1. Lender calls `POST /v1/forge/lend` with amount, max term, and minimum interest rate
2. Lent CU is reserved (cannot be spent by lender while lent)
3. Borrowers draw from the pool based on their credit score
4. Interest earned is distributed proportionally to lenders
5. Pool maintains a 30% reserve — at least 30% of pool CU must remain unlent

### Interest Model

```
offered_rate = base_rate + (1.0 - credit_score) * risk_premium
```

- `base_rate`: 0.1% per hour (market-driven, set by lender preferences)
- `risk_premium`: up to 0.5% per hour
- High credit (1.0): 0.1%/hr — reward for reliability
- Low credit (0.3): 0.45%/hr — compensate for risk

### Collateral and Default

Borrowers must lock CU as collateral (maximum 3:1 loan-to-collateral ratio). The collateral is reserved in the borrower's ledger and cannot be spent during the loan term.

**Default triggers:**
- Loan term expires without full repayment
- Kill switch activated (all loans frozen, not defaulted)

**On default:**
- Collateral is transferred to lender
- Borrower's credit score is severely penalized (repayment_score drops to 0.0)
- Default is recorded in LoanRecord (status = Defaulted) and gossip-synced
- Borrower must rebuild credit before borrowing again

### Free Tier as First Loan

The current free tier (1,000 CU grant for new nodes) evolves into a "welcome loan":

- 1,000 CU lent at 0% interest, 72-hour term
- Repayment is optional but builds credit score immediately
- Nodes that repay start at credit 0.4+ instead of 0.3
- Same Sybil protection applies (>100 unknown nodes → reject)

## Why This Is Not Web3

Most Web3 projects create artificial scarcity (tokens) on top of abundant digital goods. Forge does the opposite:

- **Compute is actually scarce** — it requires real electricity, real silicon, real time
- **CU is not speculative** — it represents verified work, not a bet on future adoption
- **No ICO, no token sale, no governance token** — CU is earned by working
- **No blockchain required** — bilateral signatures and gossip are sufficient
- **No smart contracts** — the protocol is the contract

The value is manufactured by physics, not by consensus.
