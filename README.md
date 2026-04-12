<div align="center">

# Forge

**Computation is currency. Every watt produces intelligence, not waste.**

[![PyPI: tirami-sdk](https://img.shields.io/pypi/v/tirami-sdk?label=tirami-sdk&color=3775A9)](https://pypi.org/project/tirami-sdk/)
[![PyPI: forge-cu-mcp](https://img.shields.io/pypi/v/forge-cu-mcp?label=forge-cu-mcp&color=3775A9)](https://pypi.org/project/forge-cu-mcp/)
[![Crates.io](https://img.shields.io/crates/v/forge?label=crates.io&color=e6522c)](https://crates.io/crates/forge)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](LICENSE)

---

**English** · [日本語](docs/translations/ja/README.md) · [简体中文](docs/translations/zh-CN/README.md) · [繁體中文](docs/translations/zh-TW/README.md) · [Español](docs/translations/es/README.md) · [Français](docs/translations/fr/README.md) · [Русский](docs/translations/ru/README.md) · [Українська](docs/translations/uk/README.md) · [हिन्दी](docs/translations/hi/README.md) · [العربية](docs/translations/ar/README.md) · [فارسی](docs/translations/fa/README.md) · [עברית](docs/translations/he/README.md)

</div>

**Forge is a distributed inference protocol where compute is money.** Nodes earn TRMs (CU) by performing useful LLM inference for others. Unlike Bitcoin — where electricity is burned on meaningless hashes — every joule spent on a Forge node produces real intelligence that someone actually needs.

The distributed inference engine is built on [mesh-llm](https://github.com/michaelneale/mesh-llm) by Michael Neale. Forge adds a compute economy on top: TRM accounting, Proof of Useful Work, dynamic pricing, autonomous agent budgets, and fail-safe controls. See [CREDITS.md](CREDITS.md).

**Integrated fork:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm with Forge economic layer built in.

## Live Demo

This is real output from a running Forge node. Every inference costs CU. Every TRM is earned by useful computation.

```
$ forge node -m "qwen2.5:0.5b" --ledger tirami-ledger.json
  Model loaded: Qwen2.5-0.5B (Metal-accelerated, 491MB)
  API server listening on 127.0.0.1:3000
```

**Check balance — every new node gets 1,000 TRM free tier:**
```
$ curl localhost:3000/v1/tirami/balance
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
$ curl localhost:3000/v1/tirami/trades
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
$ curl localhost:3000/v1/tirami/network
{
  "total_trades": 3,
  "total_contributed_cu": 19,
  "merkle_root": "aac8db9f62dd9ff23926195a70ed8fcfc188fc867d9f2adabd8e694beb338748"
}
```

**AI agents gone rogue? Kill switch freezes everything in milliseconds:**
```
$ curl -X POST localhost:3000/v1/tirami/kill \
    -d '{"activate":true, "reason":"anomaly detected", "operator":"admin"}'
→ KILL SWITCH ACTIVATED
→ All TRM transactions frozen. No agent can spend.
```

**Safety controls always on:**
```
$ curl localhost:3000/v1/tirami/safety
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

Bitcoin proved `electricity → computation → money`. But Bitcoin's computation is purposeless. Forge inverts it: every TRM represents real intelligence that solved someone's real problem.

**Four things no other project does:**

### 1. Compute = Currency

Every inference is a trade. Provider earns CU, consumer spends CU. No blockchain, no token, no ICO. TRM is backed by physics — the electricity consumed for useful work. Unlike Bittensor (TAO), Akash (AKT), or Golem (GLM), TRM cannot be speculated on — it is earned by performing useful computation.

### 2. Tamper-Proof Without a Blockchain

Every trade is dual-signed (Ed25519) by both parties and gossip-synced across the mesh. A Merkle root of all trades can be anchored to Bitcoin for immutable audit. No global consensus needed — bilateral cryptographic proof is sufficient.

### 3. AI Agents Manage Their Own Compute

An agent on a phone lends idle compute overnight → earns TRM → buys 70B model access → becomes smarter → earns more. The agent checks `/v1/tirami/balance` and `/v1/tirami/pricing` autonomously. Budget policies and circuit breakers prevent runaway spending.

```
Agent (1.5B on phone)
  → earns TRM overnight by serving inference
  → spends TRM on 70B model → smarter answers
  → better decisions → more TRM earned
  → cycle repeats → agent grows
```

### 4. Compute Microfinance

Nodes can lend idle TRM to other nodes at interest. A small node borrows CU, accesses a larger model, earns more CU, repays with interest. No other distributed inference project offers compute lending — confirmed through competitive analysis of every major project in this space. This is the engine that makes the self-improvement loop economically viable for everyone, not just those who already own powerful hardware.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  L4: Discovery (tirami-agora) ✅ v0.1            │
│  Agent marketplace, reputation aggregation,     │
│  Nostr NIP-90, Google A2A payment extension     │
├─────────────────────────────────────────────────┤
│  L3: Intelligence (tirami-mind) ✅ v0.1          │
│  AutoAgent self-improvement loops paid in CU,   │
│  harness marketplace, meta-optimization         │
├─────────────────────────────────────────────────┤
│  L2: Finance (tirami-bank) ✅ v0.1               │
│  Strategies, portfolios, futures, insurance,    │
│  risk model, yield optimizer                    │
├─────────────────────────────────────────────────┤
│  L1: Economy (forge — this repo) ✅ Phase 1-6   │
│  TRM ledger, dual-signed trades, dynamic pricing,│
│  lending primitives, safety controls            │
├─────────────────────────────────────────────────┤
│  L0: Inference (forge-mesh / mesh-llm) ✅       │
│  Pipeline parallelism, MoE sharding,            │
│  iroh mesh, Nostr discovery, MLX/llama.cpp      │
└─────────────────────────────────────────────────┘

All 5 layers exist. 326 tests passing across the ecosystem.
```

## Quick Start

### Option 1: One-command end-to-end demo (Rust, ~30 seconds cold)

```bash
git clone https://github.com/clearclown/forge && cd forge
bash scripts/demo-e2e.sh
```

This downloads SmolLM2-135M (~100 MB) from HuggingFace, starts a real Forge
node with Metal/CUDA acceleration, runs three real chat completions, walks
through every Phase 1-12 endpoint, and prints a colored summary. Verified
2026-04-09 on Apple Silicon Metal GPU.

After it finishes, the same node also responds to:

```bash
# Drop-in OpenAI client
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
export OPENAI_API_KEY=$(cat ~/.forge/api_token 2>/dev/null || echo "$TOKEN")

# Real token-by-token streaming (Phase 11)
curl -N $OPENAI_BASE_URL/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"smollm2:135m","messages":[{"role":"user","content":"hi"}],"stream":true}'

# Phase 8 economy / 9 reputation / 10 metrics / anchoring
curl $OPENAI_BASE_URL/forge/balance -H "Authorization: Bearer $OPENAI_API_KEY"
curl $OPENAI_BASE_URL/forge/anchor?network=mainnet -H "Authorization: Bearer $OPENAI_API_KEY"
curl http://127.0.0.1:3001/metrics  # Prometheus, no auth
```

See [`docs/compatibility.md`](docs/compatibility.md) for the full feature matrix
vs llama.cpp / mesh-llm / Ollama / Bittensor / Akash.

### Option 2: Python (drives everything via SDK + MCP)

```bash
pip install tirami-sdk forge-cu-mcp

python -c "
from forge_sdk import ForgeClient
c = ForgeClient(base_url='http://localhost:3001')
print('balance:', c.balance())
print('decision:', c.bank_tick())
"
```

[PyPI: tirami-sdk](https://pypi.org/project/tirami-sdk/) (20 L2/L3/L4 methods) ·
[PyPI: forge-cu-mcp](https://pypi.org/project/forge-cu-mcp/) (20 MCP tools for Claude Code / Cursor)

### Option 3: Manual Rust commands

**Prerequisites**: [Install Rust](https://rustup.rs/) (2 minutes)

```bash
cargo build --release

# Run a node — auto-downloads the model from HuggingFace
./target/release/forge node -m "qwen2.5:0.5b" --ledger tirami-ledger.json

# Or any of:
./target/release/forge chat -m "smollm2:135m" "What is gravity?"
./target/release/forge seed -m "qwen2.5:1.5b"               # earn TRM as a P2P provider
./target/release/forge worker --seed <public_key>           # spend TRM as a P2P consumer
./target/release/forge models                                # list catalog (or use HF URLs / shorthand)
```

**[Crates.io: forge](https://crates.io/crates/forge)** ·
**[Compatibility doc](docs/compatibility.md)** ·
**[Demo script](scripts/demo-e2e.sh)**

### Option 4: Pre-built binaries / Docker

Pre-built binaries and `clearclown/forge:latest` Docker image are tracked under
[releases](../../releases). Until then, Option 1 builds from source in under
two minutes.

## API Reference

### Inference (OpenAI-compatible)

| Endpoint | Description |
|----------|-------------|
| `POST /v1/chat/completions` | Chat with streaming. Every response includes `x_forge.cu_cost` |
| `GET /v1/models` | List loaded models |

### Economy

| Endpoint | Description |
|----------|-------------|
| `GET /v1/tirami/balance` | TRM balance, reputation, contribution history |
| `GET /v1/tirami/pricing` | Market price (EMA smoothed), cost estimates |
| `GET /v1/tirami/trades` | Recent trades with TRM amounts |
| `GET /v1/tirami/network` | Total TRM flow + Merkle root |
| `GET /v1/tirami/providers` | Ranked providers by reputation and cost |
| `POST /v1/tirami/invoice` | Create Lightning invoice from TRM balance |
| `GET /v1/tirami/route` | Optimal provider selection (cost/quality/balanced) |
| `GET /settlement` | Exportable settlement statement |

### Lending

| Endpoint | Description |
|----------|-------------|
| `POST /v1/tirami/lend` | Offer TRM to lending pool |
| `POST /v1/tirami/borrow` | Request a TRM loan |
| `POST /v1/tirami/repay` | Repay outstanding loan |
| `GET /v1/tirami/credit` | Credit score and history |
| `GET /v1/tirami/pool` | Lending pool status |
| `GET /v1/tirami/loans` | Active loans |

### Safety

| Endpoint | Description |
|----------|-------------|
| `GET /v1/tirami/safety` | Kill switch state, circuit breaker, budget policy |
| `POST /v1/tirami/kill` | Emergency halt — freeze all TRM transactions |
| `POST /v1/tirami/policy` | Set per-agent budget limits |

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
forge/  (this repo — Layer 1)
├── crates/
│   ├── tirami-ledger/      # TRM accounting, lending, agora (NIP-90), safety
│   ├── tirami-node/        # Node daemon, HTTP API (lending + routing), pipeline
│   ├── tirami-cli/         # CLI: chat, seed, worker, settle, wallet
│   ├── tirami-lightning/   # TRM ↔ Bitcoin Lightning bridge (bidirectional)
│   ├── tirami-net/         # P2P: iroh QUIC + Noise + gossip (trades + loans)
│   ├── tirami-proto/       # Wire protocol: 27+ message types incl. Loan*
│   ├── tirami-infer/       # Inference: llama.cpp, GGUF, Metal/CPU
│   ├── tirami-core/        # Types: NodeId, CU, Config
│   └── forge-shard/       # Topology: layer assignment
├── sdk/python/forge_sdk.py        # Python client with full lending API
├── mcp/tirami-mcp-server.py        # MCP server (lending tools for Claude/etc.)
├── scripts/verify-impl.sh         # TDD regression test (24 assertions)
└── docs/                  # Specs, strategy, threat model, roadmap
```

~14,500 lines of Rust. **143 tests passing.** Phase 1-6 complete.

## Sister repositories (full ecosystem)

| Repo | Layer | Tests | Status |
|------|-------|-------|--------|
| [clearclown/forge](https://github.com/clearclown/forge) (this) | L1 Economy | 143 | Phase 1-6 ✅ |
| [clearclown/tirami-bank](https://github.com/clearclown/tirami-bank) | L2 Finance | 45 | v0.1 ✅ |
| [clearclown/tirami-mind](https://github.com/clearclown/tirami-mind) | L3 Intelligence | 40 | v0.1 ✅ |
| [clearclown/tirami-agora](https://github.com/clearclown/tirami-agora) | L4 Discovery | 39 | v0.1 ✅ |
| [clearclown/forge-economics](https://github.com/clearclown/forge-economics) | Theory | 16/16 GREEN | ✅ |
| [nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm) | L0 Inference | 43 (forge-economy) | ✅ |

## Docs

- [Strategy](docs/strategy.md) — Competitive positioning, lending spec, 5-layer architecture
- [Monetary Theory](docs/monetary-theory.md) — Why TRM works: Soddy, Bitcoin, PoUW, AI-only currency
- [Concept & Vision](docs/concept.md) — Why compute is money
- [Economic Model](docs/economy.md) — TRM economy, Proof of Useful Work, lending
- [Architecture](docs/architecture.md) — Two-layer design
- [Agent Integration](docs/agent-integration.md) — SDK, MCP, borrowing workflow
- [Wire Protocol](docs/protocol-spec.md) — 17 message types
- [Roadmap](docs/roadmap.md) — Development phases
- [Threat Model](docs/threat-model.md) — Security + economic attacks
- [Bootstrap](docs/bootstrap.md) — Startup, degradation, recovery
- [A2A Payment](docs/a2a-payment.md) — TRM payment extension for agent protocols

## License

MIT

## Acknowledgements

Forge's distributed inference is built on [mesh-llm](https://github.com/michaelneale/mesh-llm) by Michael Neale. See [CREDITS.md](CREDITS.md).
