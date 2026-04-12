# Forge — Architecture

## Overview

Forge is a two-layer system: **inference** and **economy**.

The inference layer handles model distribution, mesh networking, and API serving. It is built on [mesh-llm](https://github.com/michaelneale/mesh-llm).

The economy layer handles TRM accounting, trade recording, pricing, and agent budgets. This is Forge's original contribution.

```
┌─────────────────────────────────────────────────┐
│  SDK / Integration Boundary                     │
│  Any client can embed tirami-node as a library   │
│  Third-party agents, dashboards, adapters       │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Economic Layer (Forge-original)                │
│                                                  │
│  ┌──────────────┐ ┌──────────┐ ┌─────────────┐ │
│  │ tirami-ledger │ │ pricing  │ │ agent       │ │
│  │ TRM trades    │ │ supply/  │ │ budgets     │ │
│  │ reputation   │ │ demand   │ │ /v1/tirami/* │ │
│  │ yield        │ │          │ │             │ │
│  └──────────────┘ └──────────┘ └─────────────┘ │
│                                                  │
│  ┌──────────────┐ ┌──────────────────────────┐  │
│  │ forge-verify │ │ forge-bridge (optional)  │  │
│  │ dual-sign    │ │ TRM ↔ BTC Lightning      │  │
│  │ gossip sync  │ │ TRM ↔ stablecoin         │  │
│  └──────────────┘ └──────────────────────────┘  │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Inference Layer (mesh-llm-derived)             │
│                                                  │
│  ┌────────────┐ ┌───────────┐ ┌──────────────┐ │
│  │ iroh mesh  │ │ llama.cpp │ │ OpenAI API   │ │
│  │ QUIC+Noise │ │ pipeline  │ │ /v1/chat/    │ │
│  │ Nostr disc │ │ MoE shard │ │ completions  │ │
│  └────────────┘ └───────────┘ └──────────────┘ │
└─────────────────────────────────────────────────┘
```

## Inference Layer (mesh-llm)

The inference layer is responsible for:

- **Mesh networking**: iroh-based QUIC connections with Noise encryption
- **Peer discovery**: Nostr relays for public meshes, mDNS for LAN
- **Model distribution**: Pipeline parallelism for dense models, expert sharding for MoE
- **Inference execution**: llama.cpp via llama-server and rpc-server subprocesses
- **API serving**: OpenAI-compatible `/v1/chat/completions` and `/v1/models`

Forge inherits all of this from mesh-llm. The inference layer does not know about CU, trades, or pricing.

## Economic Layer (Forge)

The economic layer sits above inference and is responsible for:

### tirami-ledger — The Economic Engine

```rust
pub struct ComputeLedger {
    balances: HashMap<NodeId, NodeBalance>,
    work_log: Vec<WorkUnit>,
    trade_log: Vec<TradeRecord>,
    price: MarketPrice,
}
```

Core responsibilities:
- Track per-node TRM balance (contributed, consumed, reserved)
- Record every inference trade (provider, consumer, TRM amount, tokens)
- Compute dynamic market prices from supply/demand
- Apply yield to contributing nodes
- Export settlement statements for off-protocol bridges
- Persist snapshots to disk with HMAC-SHA256 integrity

### forge-verify — Proof of Useful Work (target)

Ensures TRM claims are legitimate:
- Dual-sign protocol: both provider and consumer sign each TradeRecord
- Gossip sync: signed trades propagate across the network
- Verification: any node can validate both signatures
- Fraud detection: mismatched or unsigned trades are rejected

### forge-bridge — External Settlement (optional)

Converts TRM to external value for operators who need it:
- Bitcoin Lightning: TRM → msats via configurable exchange rate
- Stablecoin: TRM → USDC/USDT via adapter
- Fiat: TRM → bank transfer via operator dashboard

The bridge layer is outside the core protocol. Different operators can use different bridges.

### API Surface

| Route | Layer | Description |
|-------|-------|-------------|
| `POST /v1/chat/completions` | Inference + Economy | Run inference, record TRM trade |
| `GET /v1/models` | Inference | List loaded models |
| `GET /v1/tirami/balance` | Economy | TRM balance, reputation |
| `GET /v1/tirami/pricing` | Economy | Market price, cost estimates |
| `GET /status` | Economy | Market price, network stats, recent trades |
| `GET /topology` | Inference | Model manifest, peers, shard plan |
| `GET /settlement` | Economy | Exportable trade history |
| `GET /health` | Inference | Basic health check |

## Data Flow

### Inference with TRM Accounting

```
Consumer sends request
    ↓
API receives POST /v1/chat/completions
    ↓
Ledger checks: can_afford(consumer, estimated_cost)?
    ↓ yes
Inference layer executes (llama-server / rpc-server)
    ↓
Tokens stream back to consumer
    ↓
Ledger records trade:
  - provider.contributed += cu_cost
  - consumer.consumed += cu_cost
  - trade_log.push(TradeRecord)
    ↓
Response includes x_forge: { cu_cost, effective_balance }
```

### Settlement Export

```
Operator runs: forge settle --hours 24
    ↓
API reads trade_log for time window
    ↓
Aggregates per-node: gross_earned, gross_spent, net_cu
    ↓
Exports JSON statement with optional reference price
    ↓
Operator uses bridge adapter to convert net TRM to BTC/fiat
```

## Security Model

```
Layer 0: Bitcoin mainchain    ← Optional anchoring (future)
Layer 1: Dual signatures      ← Provider + consumer sign each trade
Layer 2: HMAC-SHA256 ledger   ← Local integrity protection
Layer 3: iroh (QUIC + Noise)  ← Transport encryption
Layer 4: Inference execution  ← Model runs locally on provider
```

Each layer protects against different threats:
- Layer 4: Model integrity (GGUF hash verification)
- Layer 3: Transport confidentiality (eavesdropping)
- Layer 2: Local tampering (file modification)
- Layer 1: Network fraud (fake TRM claims)
- Layer 0: Historical immutability (optional Bitcoin anchor)

## Crate Dependencies

```
tirami-core ← shared types (NodeId, CU, Config)
    ↑
tirami-ledger ← economic engine (trades, pricing, yield)
    ↑
tirami-lightning ← external bridge (LDK wallet, CU↔sats)
    ↑
tirami-node ← orchestrator (HTTP API, pipeline, ledger integration)
    ↑
tirami-cli ← reference CLI (chat, seed, worker, settle)

tirami-net ← P2P transport (iroh, QUIC, Noise, mDNS)
tirami-proto ← wire messages (bincode, 14 payload types)
tirami-infer ← inference engine (llama.cpp, GGUF loader)
forge-shard ← topology planner (layer assignment, rebalancing)
```
