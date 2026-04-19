<div align="center">

# Tirami

**Computation is currency. Every watt produces intelligence, not waste.**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

**English** · [日本語](docs/translations/ja/README.md) · [简体中文](docs/translations/zh-CN/README.md) · [繁體中文](docs/translations/zh-TW/README.md) · [Español](docs/translations/es/README.md) · [Français](docs/translations/fr/README.md) · [Русский](docs/translations/ru/README.md) · [Українська](docs/translations/uk/README.md) · [हिन्दी](docs/translations/hi/README.md) · [العربية](docs/translations/ar/README.md) · [فارسی](docs/translations/fa/README.md) · [עברית](docs/translations/he/README.md)

</div>

**Tirami is a distributed inference protocol where compute is money.** Nodes earn TRM (Tirami Resource Merit) by performing useful LLM inference for others. Unlike Bitcoin — where electricity is burned on meaningless hashes — every joule spent on a Tirami node produces real intelligence that someone actually needs.

The distributed inference engine is built on [mesh-llm](https://github.com/michaelneale/mesh-llm) by Michael Neale. Tirami adds a compute economy on top: TRM accounting, Proof of Useful Work, dynamic pricing, autonomous agent budgets, and fail-safe controls. See [CREDITS.md](CREDITS.md).

**Integrated fork:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm with Tirami economic layer built in.

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

Before anything else, here is exactly what works and what does not. Tirami is MIT-licensed open-source software, **not a token sale**. No ICO, no pre-mine, no team treasury, no airdrop. TRM is compute accounting (1 TRM = 10⁹ FLOP), not a financial product — see [`SECURITY.md § Secondary Markets`](SECURITY.md#secondary-markets--third-party-tokenization).

### ✅ Functional today (1 192 Rust tests + 15 Solidity tests, verified)

- HTTP OpenAI-compatible chat with automatic P2P forwarding to a connected peer (`forward_chat_to_peer`, Phase 19).
- Dual-signed `SignedTradeRecord` via iroh-QUIC P2P with 128-bit nonce replay protection (`execute_signed_trade`).
- `TradeAcceptDispatcher` routes counter-sign messages to the matching in-flight inference task (Phase 18.5-pt3).
- Collusion detector + stake-slashing loop running every `slashing_interval_secs` (Phase 17 Wave 1.3).
- Governance proposals with a 21-entry mutable whitelist and an 18-entry constitutional-parameter immutable list (Phase 18.1).
- Welcome loan, stake pool, referral bonuses, credit scoring, dynamic market pricing (EMA-smoothed).
- Peer auto-discovery via `PriceSignal.http_endpoint` on the gossip wire (Phase 19 Tier C).
- PersonalAgent auto-configured on `tirami start` (Phase 18.5-pt3e), with tick-loop observability.
- Prometheus `/metrics` endpoint using the `tirami_*` prefix.
- Base Sepolia/mainnet deploy `Makefile` targets — sepolia is free to run, mainnet is gated (see below).

### 🟡 Scaffolded (spec + types exist; production wiring pending)

- zkML proof-of-inference: `tirami-zkml-bench` has a `MockBackend` only. Real `ezkl` / `risc0` backends land in Phase 20+. Default `ProofPolicy = Optional` (Phase 19) — proofs are accepted and rewarded when supplied, but trades without proofs are still valid during the rollout.
- ML-DSA (Dilithium) post-quantum hybrid signatures: struct + verify path exist, `Config::pq_signatures = false` by default (blocked on iroh 0.97 dep chain).
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): `tirami-attestation` scaffold only.
- Daemon-mode worker gossip-recv loop ([issue #88](https://github.com/clearclown/tirami/issues/88)): manual `peer.url` override in `POST /v1/tirami/agent/task` still works.

### ❌ Not done (required before public mainnet)

- External security audit (Phase 17 Wave 3.3 requirement). Candidates: Trail of Bits, Zellic, Open Zeppelin, Least Authority.
- Base L2 mainnet deploy. The `make deploy-base-mainnet` target *refuses* to run unless `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + operator types `i-accept-responsibility` at an interactive prompt. See [`repos/tirami-contracts/Makefile`](repos/tirami-contracts/Makefile) and [`docs/deployments/README.md`](docs/deployments/README.md).
- Live bug bounty with a real PGP key (currently a documented placeholder in [`SECURITY.md`](SECURITY.md)).
- ≥ 30-day stable operation on Base Sepolia + ≥ 7-day stress-test on a 10+ node testnet.

Full tier roadmap (OSS preview → invited testnet → open testnet → mainnet): [`docs/release-readiness.md`](docs/release-readiness.md).

---

## Live Demo

Tirami is the **GPU Airbnb × AI Agent Economy**: spare compute earns TRM rent; AI agents are the tenants. Real output from a running Tirami node:

```
$ tirami start                                       # Phase 15 — one-command bootstrap
🔑 Generated new node key at /Users/ablaze/.tirami/node.key

╔══════════════════════════════════════════════════════════════╗
║         🌱 Tirami — GPU Airbnb × AI Agent Economy            ║
╚══════════════════════════════════════════════════════════════╝

   Data dir:  /Users/ablaze/.tirami
   Model:     qwen2.5:0.5b
   API:       http://127.0.0.1:3000

✅ Model loaded
🟢 Tirami node is running. Press Ctrl-C to stop.
```

**See who's on the market — PeerRegistry (Phase 14.1):**
```
$ curl localhost:3000/v1/tirami/peers
{ "count": 1, "peers": [{
    "node_id": "48b5c0f2...", "price_multiplier": 1.0,
    "available_cu": 1000, "audit_tier": "Unverified",
    "models": ["qwen2.5-0.5b-instruct-q4_k_m"]
}] }
```

**Ask the Ledger-as-Brain who it would pick (Phase 14.2):**
```
$ curl localhost:3000/v1/tirami/schedule -d '{"model_id":"qwen2.5-0.5b-instruct-q4_k_m","max_tokens":100}'
{ "provider": "48b5c0f2...", "estimated_trm_cost": 100 }
```

**Run inference billed to a specific agent — bilateral trade (Phase 14.3):**
```
$ curl localhost:3000/v1/chat/completions \
    -H "X-Tirami-Node-Id: 06d91e56..." \
    -d '{"messages":[{"role":"user","content":"Say hello in Japanese"}]}'
{
  "choices": [{"message": {"content": "こんにちは！"}}],
  "x_tirami": {"trm_cost": 9, "effective_balance": 1009}
}
```

**Trade record now includes FLOP measurement (Phase 15.3):**
```
$ curl localhost:3000/v1/tirami/trades
[{ "provider": "48b5c0f2...", "consumer": "06d91e56...",
   "trm_amount": 9, "tokens_processed": 9, "flops_estimated": 1040449536 }]
```

Every response includes `x_tirami` — **the cost in TRM** + the remaining balance. The
`flops_estimated` field anchors the principle "1 TRM = 10⁹ FLOP" with **measured data**.
Provider earns, consumer spends, physics bookkept.

**Check tokenomics — Bitcoin-inspired supply curve:**
```
$ tirami su supply
  Total Supply Cap:    21,000,000,000 TRM
  Total Minted:        0
  Supply Factor:       1.0 (genesis)
  Current Epoch:       0
  Yield Rate:          0.001/hr
```

**Every trade has a Merkle root — anchorable to Bitcoin for immutable proof:**
```
$ curl localhost:3000/v1/tirami/network
{
  "total_trades": 3,
  "total_contributed_cu": 19,
  "merkle_root": "aac8db9f...38748"
}
```

**AI agents gone rogue? Kill switch freezes everything in milliseconds:**
```
$ curl -X POST localhost:3000/v1/tirami/kill \
    -d '{"activate":true, "reason":"anomaly detected", "operator":"admin"}'
→ KILL SWITCH ACTIVATED
→ All TRM transactions frozen. No agent can spend.
```

## Why Tirami Exists

```
Bitcoin:  electricity  →  meaningless SHA-256  →  BTC
Tirami:   electricity  →  useful LLM inference →  TRM
```

Bitcoin proved `electricity → computation → money`. But Bitcoin's computation is purposeless. Tirami inverts it: every TRM represents real intelligence that solved someone's real problem.

**Phase 15 restated the whole thing in one line**:

> GPU の Airbnb × AI Agent Economy. 余っている GPU が家賃 (TRM) を生み、AI エージェントが借主になる。

```
You have a Mac sitting idle          An AI agent needs to think
        │                                         │
        ▼                                         ▼
   [ tirami start ]      ←  TRM  ←        [ agent.balance() ]
   provides inference                     pays for inference
        │                                         │
        ▼                                         ▼
   Earns TRM (= Airbnb rent)              Gets answer, keeps working
```

Every inference is measured in **FLOPs**, not just tokens:
`1 TRM = 10⁹ FLOP of verified useful computation` (Phase 15.3 anchors this
principle with measured data on every trade record).

**Five things no other project does:**

### 1. Compute = Currency (21B Supply Cap)

Every inference is a trade. Provider earns TRM, consumer spends TRM. No blockchain, no token, no ICO. TRM is backed by physics — the electricity consumed for useful work. Bitcoin-inspired tokenomics: 21 billion TRM supply cap, halving epochs, staking with multipliers, and referral bonuses for network growth.

### 2. Tamper-Proof Without a Blockchain

Every trade is dual-signed (Ed25519) by both parties and gossip-synced across the mesh. A Merkle root of all trades can be anchored to Bitcoin via OP_RETURN for immutable audit. No global consensus needed — bilateral cryptographic proof is sufficient.

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

Nodes can lend idle TRM to other nodes at interest. A small node borrows TRM, accesses a larger model, earns more TRM, repays with interest. This is the engine that makes the self-improvement loop economically viable for everyone, not just those who already own powerful hardware.

### 5. Ledger-as-Brain: scheduling IS economic decision (Phase 14+)

The ledger doesn't just track trades — it *decides* them. A single call
`begin_inference(model, tokens)` picks the best provider from the gossip-synced
PeerRegistry, reserves TRM, executes the inference, and settles — all atomically.

```
                   ┌─────────────────────────────────────────┐
                   │   ComputeLedger (Ledger-as-Brain)        │
                   │                                          │
                   │   PeerRegistry  ⇄  select_provider()     │
                   │     │                     │               │
                   │     │          begin_inference()         │
                   │     │                     │               │
                   │     │                     ▼               │
                   │     │            InferenceTicket          │
                   │     │                     │               │
                   │     │            settle_inference()       │
                   │     │                     │               │
                   │     └─── record_audit_result() ◄──────┐   │
                   │              (Phase 14.3 audit tier)  │   │
                   └───────────────────────┬───────────────┼───┘
                                           │               │
                ┌──────── gossip ──────────┤               │
                │  PriceSignalGossip       │               │
                │  TradeGossip             │               │
                │  AuditChallenge/Response─┼───────────────┘
                │  ReputationGossip        │
                └──────────────────────────┘
```

Every node's ledger sees the same economic reality within seconds. The
scheduler's decisions feed back into reputation, which feeds back into future
scheduling. Price discovery, capacity balancing, trust all emerge from this
single loop.

## Architecture

```
                          Humans & AI agents
                                 │
                                 ▼
┌─────────────────────────────────────────────────┐
│  L4: Discovery (tirami-agora) ✅                 │
│  Agent marketplace, reputation, Nostr NIP-90,   │
│  governance (stake-weighted voting)             │
├─────────────────────────────────────────────────┤
│  L3: Intelligence (tirami-mind) ✅               │
│  AutoAgent self-improvement loops paid in TRM,  │
│  harness marketplace, meta-optimization         │
├─────────────────────────────────────────────────┤
│  L2: Finance (tirami-bank) ✅                    │
│  Strategies, portfolios, futures, insurance,    │
│  risk model, yield optimizer, staking           │
├─────────────────────────────────────────────────┤
│  L1: Economy (tirami — this repo) ✅ Phase 1-16 │
│  TRM ledger with Ledger-as-Brain scheduling,   │
│  dual-signed trades, dynamic pricing, lending,  │
│  tokenomics (21B cap, halving), safety,         │
│  Prometheus, FLOP measurement, audit tiers,     │
│  gossip PriceSignal, on-chain anchor loop       │
├─────────────────────────────────────────────────┤
│  L0: Inference (forge-mesh / mesh-llm) ✅       │
│  Pipeline parallelism, MoE sharding, iroh mesh, │
│  Nostr discovery, MLX/llama.cpp                 │
└─────────────────────────────────────────────────┘
                                 │
         ┌───────────────────────┘
         │  periodic 10-min batches (Phase 16)
         ▼
┌─────────────────────────────────────────────────┐
│  On-chain: tirami-contracts (Base L2, skeleton) │
│  TRM ERC-20 (21B cap) + TiramiBridge            │
│  storeBatch / mintForProvider / withdraw        │
│  Not deployed yet — in-memory MockChainClient   │
└─────────────────────────────────────────────────┘

All 5 layers are Rust across 16 workspace crates. **1 192 tests passing
+ 15 Solidity tests.** 123/123 verify-impl GREEN. Phase 17 shipped 24
security primitives across 4 waves for public-network readiness; Phase
18-19 layered on Constitutional parameters, stake-required mining, the
zkML `ProofPolicy` ratchet, peer HTTP auto-discovery, and a gated
mainnet deploy path — see [`docs/release-readiness.md`](docs/release-readiness.md) for the tier A-D roadmap.

Mainnet deploy is gated on external audit + 30-day Sepolia stability +
multi-sig custody + bug bounty live ([`docs/security/audit-scope.md`](docs/security/audit-scope.md)).

Phase 14-16 added unified Ledger-as-Brain scheduling, FLOP measurement,
audit challenge-response, and the on-chain anchor layer
(`tirami-anchor` + `tirami-contracts`). Phase 18.3 added
`tirami-zkml-bench` (MockBackend + ezkl/risc0/halo2 feature-gated
stubs). Phase 17 Wave 3.1 added the `tirami-attestation` scaffold.
```

## Quick Start

### Option 1: One-command end-to-end demo (~30 seconds cold)

```bash
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh
```

This downloads SmolLM2-135M (~100 MB) from HuggingFace, starts a real Tirami
node with Metal/CUDA acceleration, runs real chat completions, walks
through every Phase 1-19 endpoint, and prints a colored summary.

After it finishes, the same node also responds to:

```bash
# Drop-in OpenAI client
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
export OPENAI_API_KEY=$(cat ~/.tirami/api_token 2>/dev/null || echo "$TOKEN")

# Real token-by-token streaming
curl -N $OPENAI_BASE_URL/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"smollm2:135m","messages":[{"role":"user","content":"hi"}],"stream":true}'

# Economy / reputation / metrics / anchoring
curl $OPENAI_BASE_URL/tirami/balance -H "Authorization: Bearer $OPENAI_API_KEY"
curl $OPENAI_BASE_URL/tirami/anchors  -H "Authorization: Bearer $OPENAI_API_KEY"
curl http://127.0.0.1:3001/metrics  # Prometheus, no auth
```

Phase 19 Tier C/D enablers you can exercise in the same flow:

```bash
# Personal agent — auto-configured on `tirami start`; talk to your agent from the CLI
tirami agent status            # balance + today's earn/spend + loop state
tirami agent chat "Summarize this paper" --max-tokens 256

# HTTP → P2P forwarding — worker with no local model forwards to a seed over iroh.
# The seed runs inference, streams tokens back, and both sides counter-sign the trade.
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# Peer auto-discovery — seeds advertise their HTTP endpoint on the gossip wire
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# Mainnet deploy is gated (will refuse to run without audit clearance)
cd repos/tirami-contracts && make help
```

See [`docs/compatibility.md`](docs/compatibility.md) for the full feature matrix
vs llama.cpp / mesh-llm / Ollama / Bittensor / Akash.

### Option 2: Rust SDK + MCP (all Rust, no Python)

```bash
# SDK — async HTTP client for all Tirami endpoints
cargo add tirami-sdk

# MCP server — 40 tools for Claude Code / Cursor / ChatGPT
cargo install tirami-mcp
tirami-mcp  # stdio JSON-RPC server
```

### Option 3: Manual Rust commands

**Prerequisites**: [Install Rust](https://rustup.rs/) (2 minutes)

```bash
cargo build --release

# Run a node — auto-downloads the model from HuggingFace
./target/release/tirami node -m "qwen2.5:0.5b" --ledger tirami-ledger.json

# Or any of:
./target/release/tirami chat -m "smollm2:135m" "What is gravity?"
./target/release/tirami seed -m "qwen2.5:1.5b"               # earn TRM as a P2P provider
./target/release/tirami worker --seed <public_key>            # spend TRM as a P2P consumer
./target/release/tirami models                                 # list catalog
./target/release/tirami su supply                              # check tokenomics
./target/release/tirami su stake 10000 90d                     # stake TRM for 90 days (2.0× multiplier)
```

## API Reference

### Inference (OpenAI-compatible)

| Endpoint | Description |
|----------|-------------|
| `POST /v1/chat/completions` | Chat with streaming. Every response includes `x_tirami.trm_cost`. If the local engine has no model loaded, the request is forwarded to a connected peer over P2P (`forward_chat_to_peer`, Phase 19 Tier C) and a dual-signed trade is recorded on settlement. |
| `POST /v1/tirami/agent/task` | Synchronous agent dispatch — classifies local vs. remote, picks a provider via `select_provider` + `peer_http_endpoint`, returns the decision (`run_local` / `run_remote` / `ask_user`). Phase 18.5-pt3. |
| `GET /v1/tirami/agent/status` | Personal agent state (balance, today's tally, preferences, tick-loop counters). Phase 18.5-pt3. |
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

### Tokenomics (Tirami Su)

| Endpoint | Description |
|----------|-------------|
| `GET /v1/tirami/su/supply` | Supply cap, minted, epoch, yield rate |
| `POST /v1/tirami/su/stake` | Lock TRM for staking (7d/30d/90d/365d multipliers) |
| `POST /v1/tirami/su/unstake` | Unlock staked TRM |
| `POST /v1/tirami/su/refer` | Register a referral (100 TRM bonus) |
| `GET /v1/tirami/su/referrals` | Referral stats |

### Governance

| Endpoint | Description |
|----------|-------------|
| `POST /v1/tirami/governance/propose` | Create a governance proposal |
| `POST /v1/tirami/governance/vote` | Cast a stake-weighted vote |
| `GET /v1/tirami/governance/proposals` | List active proposals |
| `GET /v1/tirami/governance/tally/{id}` | Tally votes for a proposal |

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

### Observability

| Endpoint | Description |
|----------|-------------|
| `GET /metrics` | Prometheus/OpenMetrics (20+ gauges including tokenomics, governance) |
| `GET /v1/tirami/anchor` | Bitcoin OP_RETURN anchor payload (40-byte FRGE header + Merkle root) |
| `GET /v1/tirami/collusion/{hex}` | Collusion score for a node (Tarjan SCC + volume spike) |

## Safety Design

AI agents spending compute autonomously is powerful but dangerous. Tirami has five safety layers:

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
| 1944-1971 | Bretton Woods | USD pegged to gold |
| 1971-present | Petrodollar | Oil demand + military power |
| 2009-present | Bitcoin | Energy on SHA-256 (useless work) |
| **Now** | **Compute Standard** | **Energy on LLM inference (useful work)** |

A room full of Mac Minis running Tirami is an apartment building — generating yield by performing useful work while the owner sleeps.

## Project Structure

```
tirami/  (this repo — all 5 layers, 16 Rust crates)
├── crates/
│   ├── tirami-ledger/       # TRM accounting, lending, tokenomics, staking,
│   │                        # governance whitelist + constitutional params,
│   │                        # collusion, slashing, PeerRegistry, audit,
│   │                        # ProofPolicy ratchet, nonce replay protection
│   ├── tirami-node/         # Node daemon, HTTP API (70+ endpoints), pipeline,
│   │                        # TradeAcceptDispatcher, forward_chat_to_peer,
│   │                        # agent_loop, anchor/audit/price-signal loops
│   ├── tirami-cli/          # CLI: chat, seed, worker, start, settle, wallet, su, agent
│   ├── tirami-sdk/          # Rust async HTTP client (60+ methods)
│   ├── tirami-mcp/          # Rust MCP server (44 tools for Claude / Cursor)
│   ├── tirami-bank/         # L2: Strategies, portfolios, futures, insurance, risk
│   ├── tirami-mind/         # L3: PersonalAgent, self-improvement, federated training
│   ├── tirami-agora/        # L4: Agent marketplace, reputation, NIP-90
│   ├── tirami-anchor/       # Phase 16: periodic Merkle-root anchor to on-chain
│   ├── tirami-lightning/    # TRM ↔ Bitcoin Lightning bridge (bidirectional)
│   ├── tirami-net/          # P2P: iroh QUIC + Noise + gossip, ASN rate-limit
│   ├── tirami-proto/        # Wire protocol: 30+ message types
│   ├── tirami-infer/        # Inference: llama.cpp, GGUF, Metal/CPU
│   ├── tirami-core/         # Types: NodeId, TRM, Config, PriceSignal (+ http_endpoint)
│   ├── tirami-shard/        # Topology: layer assignment
│   ├── tirami-zkml-bench/   # zkML benchmark harness (MockBackend + ezkl/risc0/halo2 stubs, Phase 18.3)
│   └── tirami-attestation/  # TEE attestation scaffold (Apple SE / NVIDIA H100 CC, Phase 17 Wave 3.1)
├── repos/tirami-contracts/  # Foundry workspace for TRM ERC-20 + TiramiBridge
│   ├── src/                 # 15 passing Solidity tests
│   └── Makefile             # Base Sepolia deploy + gated mainnet (AUDIT_CLEARANCE interlock)
├── scripts/verify-impl.sh   # TDD conformance (123 assertions)
└── docs/                    # Specs, whitepaper, threat model, roadmap, release-readiness
```

~25,000 lines of Rust. **1 192 tests passing** + 15 Solidity tests. Phase 1-19 complete.

## Ecosystem

| Repo | Layer | Tests | Status |
|------|-------|-------|--------|
| [clearclown/tirami](https://github.com/clearclown/tirami) (this) | L1-L4 | 1 192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | Theory | 16/16 verify-audit GREEN | Spec §1-§25, chapters §1-§18, papers PDF + arXiv tarball |
| [repos/tirami-contracts](https://github.com/clearclown/tirami/tree/main/repos/tirami-contracts) (in-tree) | On-chain | 15 forge tests | TRM ERC-20 + TiramiBridge, mainnet deploy gated (see `Makefile`) |
| [nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm) | L0 Inference | 646 | forge-economy port ✅ |
| clearclown/tirami-bank | L2 Finance | archived | Superseded by `crates/tirami-bank/` |
| clearclown/tirami-mind | L3 Intelligence | archived | Superseded by `crates/tirami-mind/` |
| clearclown/tirami-agora | L4 Discovery | archived | Superseded by `crates/tirami-agora/` |

## Docs

### Vision & strategy
- [Whitepaper](docs/whitepaper.md) — 16-section protocol spec (read top-to-bottom in one sitting)
- [Release Readiness](docs/release-readiness.md) — Tier A–D tier roadmap, what's ready now vs after audit
- [Constitution](docs/constitution.md) — 11 articles + amendment log, the governance whitelist doctrine
- [Killer-App](docs/killer-app.md) — product commitment: "My AI runs on my Mac. And yours. And theirs."
- [Public API Surface](docs/public-api-surface.md) — 5 public crates, 12 internal, stability contract
- [zkML Strategy](docs/zkml-strategy.md) — `ProofPolicy` rollout, backend evaluation (ezkl / risc0 / halo2)
- [Strategy](docs/strategy.md) — Competitive positioning, lending spec, 5-layer architecture
- [Monetary Theory](docs/monetary-theory.md) — Why TRM works: Soddy, Bitcoin, PoUW, AI-only currency
- [Concept & Vision](docs/concept.md) — Why compute is money
- [Roadmap](docs/roadmap.md) — Development phases

### Protocol
- [Economic Model](docs/economy.md) — TRM economy, Proof of Useful Work, lending
- [Architecture](docs/architecture.md) — Two-layer design (inference × economy)
- [Wire Protocol](docs/protocol-spec.md) — 30+ message types
- [Agent Integration](docs/agent-integration.md) — SDK, MCP, borrowing workflow
- [A2A Payment](docs/a2a-payment.md) — TRM payment extension for agent protocols
- [BitVM Design](docs/bitvm-design.md) — Optimistic verification via fraud proofs

### Security & operations
- [Threat Model](docs/threat-model.md) — Security + economic attacks (T1-T17)
- [Security Policy](SECURITY.md) — Reporting vulnerabilities, secondary-market disclaimer, mainnet deploy gate
- [Operator Guide](docs/operator-guide.md) — How to run a node in production
- [Bootstrap](docs/bootstrap.md) — Startup, degradation, recovery
- [Compatibility](docs/compatibility.md) — llama.cpp / mesh-llm / Ollama / Bittensor comparison
- [Deployments Record](docs/deployments/README.md) — On-chain deploy history (empty until Sepolia ship)

### Developer
- [Developer Guide](docs/developer-guide.md) — How to contribute
- [FAQ](docs/faq.md) — Common questions
- [Migration Guide](docs/migration-guide.md) — From llama-server / Ollama / Bittensor

## License

MIT. See [`LICENSE`](LICENSE).

## Not an investment — secondary-market disclaimer

TRM is **compute accounting**, not a financial product. The
protocol maintainers do not sell, promote, or speculate on TRM.
Because the code is MIT-licensed open source, anyone anywhere may
— without the maintainers' knowledge, consent, or endorsement —
bridge, list, trade, or derive TRM on secondary markets. If you
choose to hold or trade TRM as a store of value, you take on all
associated risk yourself.

- No ICO, no pre-sale, no airdrop, no private round.
- No revenue share from third-party markets.
- Base mainnet deploy is **audit-gated** (see
  [`docs/release-readiness.md`](docs/release-readiness.md) Tier D
  and the `deploy-base-mainnet` target in
  [`repos/tirami-contracts/Makefile`](repos/tirami-contracts/Makefile)).

Full text of the disclaimer is in
[`SECURITY.md`](SECURITY.md#secondary-markets--third-party-tokenization).

## Acknowledgements

Tirami's distributed inference is built on [mesh-llm](https://github.com/michaelneale/mesh-llm) by Michael Neale. See [CREDITS.md](CREDITS.md).
