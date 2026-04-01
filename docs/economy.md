# Forge — Economic Model

## Compute Standard (計算本位制)

Throughout history, money has been backed by physical scarcity:

| Era | Standard | Backing |
|---|---|---|
| Ancient | Gold/Silver | Precious metals |
| 1944-1971 | Bretton Woods | USD pegged to gold |
| 1971-present | Fiat / Petrodollar | Oil demand, military power, trust |
| **Forge era** | **Compute Standard** | **Electricity × Time × Silicon** |

Forge introduces a **Compute Standard**: the unit of value is backed by real energy expenditure performing useful computation. Unlike Bitcoin's Proof of Work (which wastes energy on meaningless hashes), every joule spent in Forge produces real intelligence — inference results that someone actually needs.

## Status Note

This document describes the economic direction of Forge, not just the currently shipped runtime.

Current implementation:
- seed/worker remote inference over encrypted transport
- CU-native local accounting
- persisted ledger snapshots
- settlement statement export

Planned but not yet complete in runtime:
- split-inference accounting by pipeline stage
- reserved CU holds for in-flight jobs
- agent-directed compute spending

### Why Compute = Money

Sam Altman's thesis: intelligence scales with compute. More electricity + more silicon = smarter AI. This means:

> **Compute is the most valuable commodity of the AI era.**

If you can convert electricity into intelligence, and intelligence creates economic value, then electricity → compute → intelligence → value is the most direct value chain possible. No intermediary, no central bank, no trust assumption. Physics backs the currency.

### The Interest Problem

Cryptocurrencies have a fundamental flaw: **they don't generate yield**. Bitcoin sitting in a wallet produces nothing. It's digital gold — a store of value, but not a productive asset.

Real estate generates rent. Bonds generate interest. Stocks generate dividends. These are productive assets.

**Compute resources are productive assets.** A Mac Mini sitting idle is like an empty apartment — it could be earning. When you lend it to the Forge network, it performs inference (useful work) and earns compute units. This is real yield, backed by real energy expenditure.

```
Apartment building        Mac Mini on Forge
─────────────────        ──────────────────
Asset: building           Asset: compute hardware
Cost: maintenance         Cost: electricity
Revenue: rent             Revenue: compute units
Yield: rent - maintenance Yield: units earned - electricity cost
Idle = lost income        Idle = wasted potential
```

## Transaction Model

### Compute Unit (CU)

The atomic unit of value in Forge. 1 CU represents a standardized amount of useful computation:

```rust
/// 1 CU = 1 billion FLOPs of verified inference work
/// Roughly: processing 1 token through 1 layer of a 7B model
pub const FLOPS_PER_CU: u64 = 1_000_000_000;
```

CU is **not a cryptocurrency inside the core protocol**. It is the accounting unit for contribution and consumption. The protocol itself stays CU-native. If an operator wants crypto or fiat settlement, that happens outside the protocol boundary through an exchange or payout adapter.

### Transaction Flow

#### Basic Inference Trade

```
1. Alice (phone) wants to run 13B model inference
   - She has 500 CU balance from previous contributions
   - Estimated cost: 150 CU for 256 tokens

2. Alice's agent discovers Bob's Mac Mini (idle, 16GB, M4)
   - Bob has reputation 0.95 (reliable node)
   - Bob's electricity cost: ~0.02 CU/token (low-power region)

3. Connection established (encrypted QUIC)
   - Layers 8-31 assigned to Bob
   - Alice keeps layers 0-7 locally

4. Inference proceeds:
   - Each forward pass: Alice → activation tensor → Bob → activation tensor → Alice
   - Per token: Bob spends ~0.001 kWh of electricity

5. Settlement (per token):
   - Bob's ledger: +0.6 CU (contributed computation)
   - Alice's ledger: -0.6 CU (consumed computation)
   - Recorded locally by both parties, gossip-synced

6. After 256 tokens:
   - Bob earned: 153.6 CU
   - Alice spent: 153.6 CU
   - Bob's Mac Mini earned ~$0.003 worth of future compute access
```

#### Interest Accumulation (Yield)

A node that contributes compute earns CU. These CU can later be spent when that node needs inference. But there's a twist — **compounding**:

```
Bob's Mac Mini runs 8 hours/night while Bob sleeps.
  Night 1: serves 1,000 inference requests → earns 5,000 CU
  Night 2: reputation increases → gets assigned more work → 6,200 CU
  Night 3: 7,100 CU
  ...
  Month 1: Bob has accumulated 180,000 CU

Bob's phone needs a 30B model for a complex task:
  Cost: 50,000 CU
  Bob can afford it — his Mac Mini earned it while he slept.
```

The "interest rate" is implicit:
- **Hardware depreciates** (like a building aging)
- **But demand for compute grows** (like rising rents in a growing city)
- **Net yield is positive** as long as AI demand exceeds idle supply

### Pricing Mechanism

CU prices float based on supply and demand:

```rust
pub struct MarketPrice {
    /// Base price: 1 CU per FLOPS_PER_CU of compute
    pub base_cu_per_token: f64,

    /// Supply multiplier: more idle nodes → lower price
    pub supply_factor: f64,

    /// Demand multiplier: more inference requests → higher price
    pub demand_factor: f64,
}
```

Effective price is computed locally as:

```rust
base_cu_per_token * demand_factor / supply_factor
```

Price discovery is local — each node observes its own market conditions and adjusts. No global order book. No central exchange in the protocol itself. Just P2P negotiation.

## Simple User Flows

The protocol should feel simple even if the market underneath is sophisticated. The default product surface should expose only three states to normal users:

- **Ready**: local model works immediately, even with zero balance
- **Growing**: the client found more compute and is buying remote inference
- **Earning**: idle hardware is online and accumulating CU

### Flow 1: Consumer with Zero Setup

```text
1. Install a Forge client
2. Local model starts immediately
3. User asks a question
4. Client discovers a seed or provider
5. Seed checks free tier / CU balance
6. If affordable, inference runs and text streams back
7. CU balance decreases only after completed work
```

Default UX rule: the user should never need to think about shards, token IDs, or routing. They should only see model quality, speed, and remaining compute budget.

### Flow 2: Home Contributor

```text
1. User runs `forged seed` on a Mac Mini or other always-on box
2. User enables share-idle-compute in the client or daemon config
3. The seed serves encrypted inference requests while idle
4. Completed requests are written into the persisted local ledger as trades
5. CU balance grows over time
6. The same user later spends those CU from a phone or laptop client
```

Default UX rule: earning should be opt-in, visible, and low-friction. One toggle. One balance. One activity feed.

### Flow 3: Operator / Marketplace Provider

```text
1. Operator runs one or more `forged` nodes
2. Operator keeps `--ledger forge-ledger.json` enabled for restart-safe accounting
3. Operator monitors `/status` for price, throughput, and recent trades
4. Operator exports settlement statements for a time window from `/settlement`
5. Operator optionally converts net CU exposure into external credits, stablecoins, or fiat payouts
6. External payout status is recorded outside the core protocol
```

Default UX rule: commercial features belong in adapters and dashboards, not in the wire protocol.

## Trading Window and Credit Policy

To keep onboarding simple, Forge should treat compute access as a quota problem before it becomes a finance problem:

- **Free tier**: every new node gets a small CU allowance for first-use inference
- **Spendable CU**: earned or previously credited balance usable immediately
- **Reserved CU**: budget temporarily held while an inference request is in flight
- **Settlement window**: a configurable accounting period where operators net inflows and outflows before external payout

This model keeps the protocol responsive and avoids forcing every inference to become a real-time payment event.

## CU to Crypto to Fiat: The Boundary

Forge should support a path to cash without turning the protocol itself into a blockchain project.

### Core Rule

The protocol settles in CU. Conversion to anything else is an integration concern.

### Recommended Layering

```text
Layer 0: Forge protocol
  - discovery
  - encrypted inference
  - CU accounting

Layer 1: Settlement statement
  - export auditable trade history
  - compute net CU earned / spent
  - attach operator-defined reference price

Layer 2: External payout adapter
  - prepaid credits
  - stablecoin payout
  - Lightning payout
  - bank transfer / fiat payout
```

### Concrete Bridge Flow

```text
1. Node earns CU inside Forge from completed inference trades
2. The ledger snapshot survives daemon restarts
3. Operator closes a settlement window (for example every hour or every day)
4. The operator exports a statement:
   - gross CU earned
   - gross CU spent
   - net CU
   - trade count
   - reference exchange rate for that window
5. An external adapter converts net CU into an external unit:
   - custodial app credits
   - stablecoin balance
   - bank payout amount
6. The external system marks that statement as paid
7. Forge keeps running exactly the same way; only the adapter changes
```

### Why This Matters

- No blockchain is required for the core network
- No smart contracts are required for inference settlement
- Different regions can use different payout rails
- The protocol remains useful even with zero external liquidity

### Exchange-Rate Discipline

If CU is bridged externally, pricing should be explicit and time-bounded:

- quote CU to external value per settlement window, not per packet
- separate **protocol price** (CU per token) from **market price** (USD/JPY/USDC per CU)
- publish fees and slippage at the adapter layer
- never let external payout logic change core scheduling or encryption behavior

### Why Apple Silicon

As of 2026, Apple Silicon offers the best compute-per-dollar for AI inference:

| Hardware | Memory | Cost | Memory Bandwidth | Inference $/token |
|---|---|---|---|---|
| Mac Mini M4 (24GB) | 24GB unified | ~$800 | 120 GB/s | Lowest |
| NVIDIA RTX 4090 (24GB) | 24GB VRAM | ~$1,600 | 1,008 GB/s | Fast but 2x cost |
| NVIDIA H100 (80GB) | 80GB HBM3 | ~$30,000 | 3,352 GB/s | Datacenter only |

For **memory-bound** inference (which is what token generation is), unified memory architecture means:
- No CPU↔GPU data transfer overhead
- More memory per dollar (Apple) vs more bandwidth per dollar (NVIDIA)
- Lower power consumption → lower electricity cost → higher net yield

A room full of Mac Minis is the apartment building of the AI era.

## Agent-Assisted Budgeting (future)

### The AI as Economic Client

Forge may eventually support software agents that manage their own compute budgets, but that is a later integration concern, not the current protocol boundary.

```
Traditional: Human decides → Human pays → AI executes
Future Forge: policy allows agent → agent spends CU within limits → agent executes
```

That future agent layer could:
1. **Detect** it needs more compute ("I can't run this 13B model locally")
2. **Discover** available nodes on the network
3. **Negotiate** price (CU per token, latency requirements)
4. **Execute** distributed inference across selected nodes
5. **Settle** payment in CU
6. **Evaluate** results and update node reputation

Human approval, policy limits, and adapter rules should remain available. Forge should not assume unsupervised agents as a protocol invariant.

### Self-Reinforcement Loop (future)

```
Agent is small (1.5B on phone)
  → earns CU by lending idle time (phone charges overnight)
  → spends CU to access larger model (13B across network)
  → becomes smarter
  → makes better trading decisions
  → earns more CU
  → accesses even larger model (30B)
  → ...
```

This is a possible application pattern, not something the current runtime guarantees.

## Comparison with Existing Systems

| System | Value Backing | Yield | Agent Autonomy |
|---|---|---|---|
| Bitcoin | Energy (wasted) | None | None |
| Ethereum | Energy + Utility | Staking (~4%) | Smart contracts |
| Filecoin | Storage | Storage fees | None |
| Golem | Compute | Task fees | Human-directed |
| **Forge** | **Compute (useful)** | **Inference yield** | **Agent integration as a future layer** |

## Philosophy

> Compute only becomes a trustworthy market when the trust boundary is honest.
> Forge should earn the right to talk about growth by first making split inference real.
