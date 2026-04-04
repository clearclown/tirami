# Forge: A Peer-to-Peer Compute Economy for AI Agents

*Version 0.2 — April 2026*

## Abstract

We propose a system where AI agents autonomously earn and spend compute through useful work. Unlike Bitcoin, where energy is consumed on purposeless hash computation, Forge nodes earn Compute Units (CU) by performing LLM inference — computation with intrinsic value. Every trade is dual-signed by provider and consumer, gossip-synced across the mesh, and Merkle-rooted for optional Bitcoin anchoring. No blockchain is required; bilateral cryptographic proof is sufficient.

## 1. Introduction

The AI compute economy has a fundamental problem: agents cannot pay for their own resources. When an AI agent needs more compute than its local device provides, it depends on a human to pay a cloud provider. This creates a bottleneck — the agent's capability is limited by its owner's willingness to spend.

Bitcoin solved a similar problem for digital money by proving that `electricity → computation → value` is a viable economic model. But Bitcoin's computation (SHA-256 hashing) is purposeless — it secures the ledger but produces nothing useful.

Forge inverts this: computation is useful (LLM inference), and the economic value comes directly from the utility produced.

## 2. Compute Units

### 2.1 Definition

**1 CU = 1 billion FLOPs of verified inference work.**

CU is not a cryptocurrency. It is not traded on exchanges. It has no speculative value. CU represents a claim on future compute — if you earned 1,000 CU by serving inference, you can spend those CU to receive inference from any other node.

### 2.2 Why Not Bitcoin?

| Property | CU | Bitcoin |
|----------|-----|---------|
| Value backing | Useful computation | Useless hashing |
| Settlement | Instant (local) | Minutes (Lightning/chain) |
| Transaction cost | Zero | Channel/chain fees |
| External dependency | None | Bitcoin network |
| Yield | Yes (inference) | No |

Bitcoin is available as an off-ramp for operators who need fiat liquidity. The core protocol settles in CU.

## 3. Proof of Useful Work

### 3.1 Trade Execution

Every inference creates a trade:

```
TradeRecord {
    provider:         Ed25519 public key of inference server
    consumer:         Ed25519 public key of requester
    cu_amount:        CU transferred
    tokens_processed: work performed
    timestamp:        unix milliseconds
    model_id:         model identifier
}
```

### 3.2 Dual Signatures

Both parties sign the canonical bytes of the trade:

```
provider signs → TradeProposal (sent to consumer)
consumer verifies, signs → TradeAccept (sent back)
both signatures → SignedTradeRecord (recorded in ledger)
```

A node cannot inflate its CU balance without a cooperating counterparty. The counterparty's signature proves the work was requested and received.

### 3.3 Gossip Propagation

Signed trades are broadcast to all connected peers. Receiving nodes verify both signatures and record the trade if new (SHA-256 deduplication). This creates an eventually-consistent view of trade history across the mesh.

### 3.4 Merkle Root

The ledger computes a SHA-256 Merkle tree over all trades. This root can be:
- Compared between peers to detect ledger divergence
- Anchored to Bitcoin via OP_RETURN for immutable audit trail
- Used as a compact proof of the entire trade history

## 4. Dynamic Pricing

CU prices float with supply and demand using an exponential moving average:

```
effective_price = base_cu_per_token × demand_factor / supply_factor
```

Each node observes its local market. No global order book. Prices converge naturally through gossip.

## 5. Agent Autonomy

### 5.1 The Self-Reinforcement Loop

```
Small agent (phone, 1.5B parameters)
  → lends idle compute → earns CU
  → checks /v1/forge/pricing → estimates cost
  → spends CU on 70B inference → gets smarter answer
  → makes better economic decisions → earns more CU
  → accesses larger models → capability grows
```

### 5.2 Budget API

Agents interact with the economy through standard HTTP:

```
GET  /v1/forge/balance     → "can I afford this?"
GET  /v1/forge/pricing     → "how much will it cost?"
GET  /v1/forge/providers   → "who's cheapest/best?"
POST /v1/chat/completions  → "run inference, pay CU"
```

No special SDK required. Any HTTP client works.

### 5.3 MCP Integration

Forge provides a Model Context Protocol (MCP) server. AI assistants (Claude, Cursor, etc.) can directly:
- Check their CU balance
- Estimate costs before making decisions
- Run inference and track spending
- Monitor safety status

## 6. Safety

AI agents spending autonomously is powerful but dangerous. Five layers of protection:

| Layer | Mechanism | Trigger |
|-------|-----------|---------|
| Kill Switch | Human freezes all trades | Manual activation |
| Budget Policy | Per-agent CU limits | Exceeds per-request/hourly/lifetime cap |
| Circuit Breaker | Auto-suspend node | 5 consecutive errors or 30+ spends/min |
| Velocity Detection | Rate anomaly detection | Burst spending pattern |
| Human Approval | Manual confirmation | Transaction exceeds threshold |

**Design principle:** fail-safe. If any check cannot determine safety, it denies the action.

## 7. Settlement

The protocol settles in CU. External conversion is optional:

```
Layer 0: Forge protocol → CU accounting
Layer 1: Settlement statement → exportable trade history + Merkle root
Layer 2: External bridge → CU ↔ BTC (Lightning) / CU ↔ stablecoin / CU ↔ fiat
```

## 8. Implementation

Forge is implemented in ~10,000 lines of Rust across 9 crates:
- `forge-ledger`: CU accounting, dual-signed trades, Merkle root, safety controls
- `forge-node`: HTTP API, pipeline coordinator
- `forge-net`: P2P transport (iroh QUIC + Noise encryption), gossip protocol
- `forge-proto`: 17 wire message types
- `forge-lightning`: CU ↔ Bitcoin Lightning bridge
- `forge-infer`: llama.cpp inference backend

76 tests. 2 completed security audits. MIT licensed.

Distributed inference (pipeline parallelism, MoE expert sharding) is provided by [mesh-llm](https://github.com/michaelneale/mesh-llm).

## 9. Related Work

| System | Compute | Economy | Agent Support |
|--------|---------|---------|---------------|
| Bitcoin | Useless (SHA-256) | Global PoW | None |
| Ethereum | Smart contracts | Gas fees | Limited (smart contracts) |
| Filecoin | Storage | FIL token | None |
| Golem | Batch compute | GNT token | Human-directed |
| mesh-llm | Distributed LLM | None | Blackboard only |
| **Forge** | **Useful LLM inference** | **CU (no blockchain)** | **Autonomous budget management** |

## 10. Conclusion

Forge proves that computation can be currency without a blockchain. The value comes from physics — electricity converted to intelligence — not from artificial scarcity. AI agents can participate in this economy autonomously, earning compute by serving others and spending it to become smarter. Safety controls ensure human oversight is always available.

The protocol is open (MIT). The computation is the currency. The code is the proof.

---

*Forge is built on mesh-llm by Michael Neale. See CREDITS.md for full acknowledgements.*

*Source: https://github.com/clearclown/forge*
