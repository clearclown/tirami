# Forge

> Computation is currency. Every watt produces intelligence, not waste.

**Forge is a distributed inference protocol where compute is money.** Nodes earn Compute Units (CU) by performing useful LLM inference for others. Unlike Bitcoin — where electricity is burned on meaningless hashes — every joule spent on a Forge node produces real intelligence that someone actually needs.

The distributed inference engine is built on [mesh-llm](https://github.com/michaelneale/mesh-llm) by Michael Neale. Forge adds a compute economy on top: CU accounting, Proof of Useful Work, dynamic pricing, and autonomous agent budgets. See [CREDITS.md](CREDITS.md) for full acknowledgements.

**Integrated fork:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm with Forge economic layer built in.

## Three Things That Make Forge Different

### 1. Compute = Currency (CU-Native Economy)

Every inference creates a trade. The provider earns CU, the consumer spends CU. No blockchain required — CU is backed by the physical reality of electricity consumed for useful work.

```
Bitcoin:  electricity  →  meaningless SHA-256  →  BTC
Forge:    electricity  →  useful LLM inference →  CU
```

CU stands on its own. Bitcoin, stablecoins, and fiat are optional off-ramps for operators who need external liquidity.

### 2. Tamper-Proof Without a Blockchain

Every trade is dual-signed by both parties and gossip-synced across the network. You cannot claim CU you didn't earn — the counterparty's signature proves the work happened. No global consensus needed; bilateral cryptographic proof is sufficient.

### 3. AI Agents Grow Their Own Resources

An AI agent on a phone can lend idle compute overnight, earn CU, and spend that CU to access a 70B model it could never run locally. The agent manages its own budget via `/v1/forge/balance` and `/v1/forge/pricing`. No human intervention required.

```
Agent (1.5B on phone)
  → lends idle compute overnight → earns CU
  → spends CU on 70B inference  → becomes smarter
  → makes better decisions      → earns more CU
  → ...
```

## How It Works

```
┌─────────────────────────────────────────────────┐
│  Inference Layer (mesh-llm)                     │
│  Pipeline parallelism, MoE expert sharding,     │
│  iroh mesh, Nostr discovery, OpenAI API         │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Economic Layer (Forge)                         │
│  CU ledger, TradeRecord, dual-signed proofs,    │
│  dynamic pricing, agent budgets, settlement     │
└──────────────────┬──────────────────────────────┘
                   │ optional
┌──────────────────▼──────────────────────────────┐
│  External Bridges                               │
│  CU ↔ BTC (Lightning), CU ↔ stablecoin,       │
│  CU ↔ fiat (operator adapters)                 │
└─────────────────────────────────────────────────┘
```

## Quick Start

```bash
# Build
cargo build --release

# Local inference (auto-downloads model)
forge chat -m "qwen2.5:0.5b" "What is gravity?"

# Start as a seed node (serves inference, earns CU)
forge seed -m "qwen2.5:0.5b" --ledger forge-ledger.json

# Connect as a worker (consumes inference, spends CU)
forge worker --seed <seed_public_key>

# Check CU balance and market price
curl http://127.0.0.1:3000/v1/forge/balance
curl http://127.0.0.1:3000/v1/forge/pricing

# OpenAI-compatible inference (with CU cost in response)
curl http://127.0.0.1:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"hello"}]}'

# Export settlement statement
forge settle --hours 24 --price 0.05

# Settlement with Lightning invoice
forge settle --hours 24 --pay

# List available models
forge models
```

## API

### OpenAI-Compatible

| Endpoint | Description |
|----------|-------------|
| `POST /v1/chat/completions` | Chat completions (streaming supported) |
| `GET /v1/models` | List loaded models |

Every response includes an `x_forge` extension with `cu_cost` and `effective_balance`.

### Forge Economic API

| Endpoint | Description |
|----------|-------------|
| `GET /v1/forge/balance` | CU balance, contribution, consumption, reputation |
| `GET /v1/forge/pricing` | Market price, supply/demand factors, cost estimates |
| `GET /status` | Node health, market price, recent trades |
| `GET /topology` | Model manifest, peer capabilities, shard plan |
| `GET /settlement` | Exportable settlement statement for a time window |

## Project Structure

```
forge/
├── crates/
│   ├── forge-core/        # Shared types: NodeId, CU, LayerRange, Config
│   ├── forge-ledger/      # Economic engine: CU accounting, trades, yield, pricing
│   ├── forge-lightning/   # External bridge: CU ↔ Bitcoin Lightning
│   ├── forge-node/        # Node daemon, HTTP API, pipeline coordinator
│   ├── forge-cli/         # Reference CLI: chat, seed, worker, settle
│   ├── forge-net/         # P2P transport: iroh QUIC + Noise encryption
│   ├── forge-proto/       # Wire protocol: 14 message types, bincode
│   ├── forge-infer/       # Inference: llama.cpp backend, GGUF loader
│   └── forge-shard/       # Topology: layer assignment, rebalancing
└── docs/
    ├── concept.md         # Vision: Compute Standard, why compute is money
    ├── economy.md         # CU-native economy, Proof of Useful Work, agent budgets
    ├── architecture.md    # Two-layer architecture: inference + economy
    ├── protocol-spec.md   # Wire protocol specification
    ├── roadmap.md         # Development phases
    ├── threat-model.md    # Security and economic attack analysis
    └── bootstrap.md       # Startup sequence and degradation
```

## The Idea

Every monetary system in history has been backed by something scarce:

| Era | Standard | Backing |
|-----|----------|---------|
| Ancient | Gold | Physical scarcity |
| 1944–1971 | Bretton Woods | USD pegged to gold |
| 1971–present | Petrodollar | Oil demand + military power |
| 2009–present | Bitcoin | Energy burned on SHA-256 (useless work) |
| **Forge** | **Compute Standard** | **Energy spent on LLM inference (useful work)** |

Bitcoin proved that `electricity → computation → money` works. But Bitcoin's computation is purposeless. Forge inverts this: every CU is backed by real inference that solved someone's problem. The computation has intrinsic value.

A room full of Mac Minis on the Forge network is an apartment building — earning yield by performing useful work while idle.

## Docs

- [Concept & Vision](docs/concept.md) — Why compute is money, Compute Standard
- [Economic Model](docs/economy.md) — CU-native economy, Proof of Useful Work, agent budgets
- [Architecture](docs/architecture.md) — Two-layer design: inference + economy
- [Wire Protocol](docs/protocol-spec.md) — Message types, serialization, connection lifecycle
- [Roadmap](docs/roadmap.md) — Development phases
- [Threat Model](docs/threat-model.md) — Security and economic attack analysis
- [Bootstrap](docs/bootstrap.md) — Startup sequence, degradation, recovery

## License

MIT

## Acknowledgements

Forge's distributed inference is built on [mesh-llm](https://github.com/michaelneale/mesh-llm) by Michael Neale. See [CREDITS.md](CREDITS.md).
