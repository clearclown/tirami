# Forge

> Computation is currency. Every watt produces intelligence, not waste.

**Forge is a distributed inference protocol where compute is money.** Nodes earn Compute Units (CU) by performing useful LLM inference for others. Unlike Bitcoin — where electricity is burned on meaningless hashes — every joule spent on a Forge node produces real intelligence that someone actually needs.

The distributed inference engine is built on [mesh-llm](https://github.com/michaelneale/mesh-llm) by Michael Neale. Forge adds a compute economy on top: CU accounting, Proof of Useful Work, dynamic pricing, autonomous agent budgets, and fail-safe controls. See [CREDITS.md](CREDITS.md).

**Integrated fork:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm with Forge economic layer built in.

## Live Demo

This is real output from a running Forge node. Every inference costs CU. Every CU is earned by useful computation.

```
$ forge node -m "qwen2.5:0.5b" --ledger forge-ledger.json
  Model loaded: Qwen2.5-0.5B (Metal-accelerated, 491MB)
  API server listening on 127.0.0.1:3000
```

**Check balance — every new node gets 1,000 CU free tier:**
```
$ curl localhost:3000/v1/forge/balance
{
  "effective_balance": 1000,
  "contributed": 0,
  "consumed": 0,
  "reputation": 0.5
}
```

**Ask a question — inference costs CU:**
```
$ curl localhost:3000/v1/chat/completions \
    -d '{"messages":[{"role":"user","content":"Say hello in Japanese"}]}'
{
  "choices": [{"message": {"content": "こんにちは！ (konnichiwa!)"}}],
  "usage": {"completion_tokens": 9},
  "x_forge": {
    "cu_cost": 9,
    "effective_balance": 1009
  }
}
```

Every response includes `x_forge` — **the cost of that computation in CU** and the remaining balance. The provider earned 9 CU. The consumer spent 9 CU. Physics backed every unit.

**Three inferences later — real trades on the ledger:**
```
$ curl localhost:3000/v1/forge/trades
{
  "count": 3,
  "trades": [
    {"cu_amount": 5, "tokens_processed": 5, "model_id": "qwen2.5-0.5b-instruct-q4_k_m"},
    {"cu_amount": 5, "tokens_processed": 5, "model_id": "qwen2.5-0.5b-instruct-q4_k_m"},
    {"cu_amount": 9, "tokens_processed": 9, "model_id": "qwen2.5-0.5b-instruct-q4_k_m"}
  ]
}
```

**Every trade has a Merkle root — anchorable to Bitcoin for immutable proof:**
```
$ curl localhost:3000/v1/forge/network
{
  "total_trades": 3,
  "total_contributed_cu": 19,
  "merkle_root": "aac8db9f62dd9ff23926195a70ed8fcfc188fc867d9f2adabd8e694beb338748"
}
```

**AI agents gone rogue? Kill switch freezes everything in milliseconds:**
```
$ curl -X POST localhost:3000/v1/forge/kill \
    -d '{"activate":true, "reason":"anomaly detected", "operator":"admin"}'
→ KILL SWITCH ACTIVATED
→ All CU transactions frozen. No agent can spend.
```

**Safety controls always on:**
```
$ curl localhost:3000/v1/forge/safety
{
  "kill_switch_active": false,
  "circuit_tripped": false,
  "policy": {
    "max_cu_per_hour": 10000,
    "max_cu_per_request": 1000,
    "max_cu_lifetime": 1000000,
    "human_approval_threshold": 5000
  }
}
```

## Why Forge Exists

```
Bitcoin:  electricity  →  meaningless SHA-256  →  BTC
Forge:    electricity  →  useful LLM inference →  CU
```

Bitcoin proved `electricity → computation → money`. But Bitcoin's computation is purposeless. Forge inverts it: every CU represents real intelligence that solved someone's real problem.

**Three things no other project does:**

### 1. Compute = Currency

Every inference is a trade. Provider earns CU, consumer spends CU. No blockchain, no token, no ICO. CU is backed by physics — the electricity consumed for useful work.

### 2. Tamper-Proof Without a Blockchain

Every trade is dual-signed (Ed25519) by both parties and gossip-synced across the mesh. A Merkle root of all trades can be anchored to Bitcoin for immutable audit. No global consensus needed — bilateral cryptographic proof is sufficient.

### 3. AI Agents Manage Their Own Compute

An agent on a phone lends idle compute overnight → earns CU → buys 70B model access → becomes smarter → earns more. The agent checks `/v1/forge/balance` and `/v1/forge/pricing` autonomously. Budget policies and circuit breakers prevent runaway spending.

```
Agent (1.5B on phone)
  → earns CU overnight by serving inference
  → spends CU on 70B model → smarter answers
  → better decisions → more CU earned
  → cycle repeats → agent grows
```

## Architecture

```
┌─────────────────────────────────────────────────┐
│  Inference Layer (mesh-llm)                     │
│  Pipeline parallelism, MoE expert sharding,     │
│  iroh mesh, Nostr discovery, OpenAI API         │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Economic Layer (Forge)                         │
│  CU ledger, dual-signed trades, gossip,         │
│  dynamic pricing, Merkle root, safety controls  │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Safety Layer                                   │
│  Kill switch, budget policies, circuit breakers,│
│  velocity detection, human approval thresholds  │
└──────────────────┬──────────────────────────────┘
                   │ optional
┌──────────────────▼──────────────────────────────┐
│  External Bridges                               │
│  CU ↔ BTC (Lightning), CU ↔ stablecoin        │
└─────────────────────────────────────────────────┘
```

## Quick Start

```bash
# Build
cargo build --release

# Run a node with auto-downloaded model
forged node -m "qwen2.5:0.5b" --ledger forge-ledger.json

# Chat locally
forge chat -m "qwen2.5:0.5b" "What is gravity?"

# Start a seed (P2P, earns CU)
forge seed -m "qwen2.5:0.5b" --ledger forge-ledger.json

# Connect as worker (P2P, spends CU)
forge worker --seed <public_key>

# List models
forge models
```

## API Reference

### Inference (OpenAI-compatible)

| Endpoint | Description |
|----------|-------------|
| `POST /v1/chat/completions` | Chat with streaming. Every response includes `x_forge.cu_cost` |
| `GET /v1/models` | List loaded models |

### Economy

| Endpoint | Description |
|----------|-------------|
| `GET /v1/forge/balance` | CU balance, reputation, contribution history |
| `GET /v1/forge/pricing` | Market price (EMA smoothed), cost estimates |
| `GET /v1/forge/trades` | Recent trades with CU amounts |
| `GET /v1/forge/network` | Total CU flow + Merkle root |
| `GET /v1/forge/providers` | Ranked providers by reputation and cost |
| `POST /v1/forge/invoice` | Create Lightning invoice from CU balance |
| `GET /settlement` | Exportable settlement statement |

### Safety

| Endpoint | Description |
|----------|-------------|
| `GET /v1/forge/safety` | Kill switch state, circuit breaker, budget policy |
| `POST /v1/forge/kill` | Emergency halt — freeze all CU transactions |
| `POST /v1/forge/policy` | Set per-agent budget limits |

## Safety Design

AI agents spending compute autonomously is powerful but dangerous. Forge has five safety layers:

| Layer | Mechanism | Protection |
|-------|-----------|------------|
| **Kill Switch** | Human operator freezes all trades instantly | Stops runaway agents |
| **Budget Policy** | Per-agent limits: per-request, hourly, lifetime | Caps total exposure |
| **Circuit Breaker** | Auto-trips on 5 errors or 30+ spends/min | Catches anomalies |
| **Velocity Detection** | 1-minute sliding window on spend rate | Prevents bursts |
| **Human Approval** | Transactions above threshold require human OK | Guards large spends |

Design principle: **fail-safe**. If any check cannot determine safety, it **denies** the action.

## The Idea

| Era | Standard | Backing |
|-----|----------|---------|
| Ancient | Gold | Geological scarcity |
| 1944–1971 | Bretton Woods | USD pegged to gold |
| 1971–present | Petrodollar | Oil demand + military power |
| 2009–present | Bitcoin | Energy on SHA-256 (useless work) |
| **Now** | **Compute Standard** | **Energy on LLM inference (useful work)** |

A room full of Mac Minis running Forge is an apartment building — generating yield by performing useful work while the owner sleeps.

## Project Structure

```
forge/
├── crates/
│   ├── forge-ledger/      # CU accounting, trades, pricing, safety, Merkle root
│   ├── forge-node/        # Node daemon, HTTP API, pipeline coordinator
│   ├── forge-cli/         # CLI: chat, seed, worker, settle, wallet
│   ├── forge-lightning/   # CU ↔ Bitcoin Lightning bridge
│   ├── forge-net/         # P2P: iroh QUIC + Noise + gossip
│   ├── forge-proto/       # Wire protocol: 17 message types
│   ├── forge-infer/       # Inference: llama.cpp, GGUF, Metal/CPU
│   ├── forge-core/        # Types: NodeId, CU, Config
│   └── forge-shard/       # Topology: layer assignment
└── docs/                  # Specs, threat model, roadmap
```

~10,000 lines of Rust. 76 tests. 2 security audits completed.

## Docs

- [Concept & Vision](docs/concept.md) — Why compute is money
- [Economic Model](docs/economy.md) — CU economy, Proof of Useful Work
- [Architecture](docs/architecture.md) — Two-layer design
- [Wire Protocol](docs/protocol-spec.md) — 17 message types
- [Roadmap](docs/roadmap.md) — Development phases
- [Threat Model](docs/threat-model.md) — Security + economic attacks
- [Bootstrap](docs/bootstrap.md) — Startup, degradation, recovery

## License

MIT

## Acknowledgements

Forge's distributed inference is built on [mesh-llm](https://github.com/michaelneale/mesh-llm) by Michael Neale. See [CREDITS.md](CREDITS.md).
