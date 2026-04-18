# Tirami — Architecture

## Overview

Tirami is a two-layer system: **inference** and **economy**.

The inference layer handles model distribution, mesh networking, and API serving. It is built on [mesh-llm](https://github.com/michaelneale/mesh-llm).

The economy layer handles TRM accounting, trade recording, pricing, and agent budgets. This is Tirami's original contribution.

```
┌─────────────────────────────────────────────────┐
│  SDK / Integration Boundary                     │
│  Any client can embed tirami-node as a library   │
│  Third-party agents, dashboards, adapters       │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Economic Layer (Tirami-original)                │
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

Tirami inherits all of this from mesh-llm. The inference layer does not know about TRM, trades, or pricing.

## Economic Layer (Tirami)

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
- Record every inference trade (provider, consumer, TRM amount, tokens processed)
- Compute dynamic market prices from supply/demand
- Apply yield to contributing nodes
- Export settlement statements for off-protocol bridges
- Persist snapshots to disk with HMAC-SHA256 integrity

### forge-verify — Proof of Useful Work (target)

Ensures trade claims are legitimate:
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
| `POST /v1/chat/completions` | Inference + Economy | Run inference, record trade |
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
  - provider.contributed += trm_cost
  - consumer.consumed += trm_cost
  - trade_log.push(TradeRecord)
    ↓
Response includes x_tirami: { trm_cost, effective_balance }
```

### Settlement Export

```
Operator runs: tirami settle --hours 24
    ↓
API reads trade_log for time window
    ↓
Aggregates per-node: gross_earned, gross_spent, net_trm
    ↓
Exports JSON statement with optional reference price
    ↓
Operator uses bridge adapter to convert net TRM to BTC/fiat
```

## Security Model

```
Layer -1: External audit            ← Gated (Wave 3.3 scoped)
Layer 0:  On-chain anchor (Base L2) ← Merkle root of trade batch, 10-min interval
Layer 1:  Dual signatures + nonce   ← Provider + consumer sign; 128-bit replay nonce
Layer 2:  HMAC-SHA256 ledger        ← Local file-integrity protection
Layer 3:  iroh (QUIC + Noise)       ← Transport encryption
Layer 4:  Inference execution       ← Model runs locally on provider
Layer 5:  Hardware attestation      ← Optional premium tier (Apple SE / NVIDIA CC / SGX)
```

Each layer protects against different threats:
- Layer 5: Hardware root of trust (optional; attested providers get 5× audit-tier speed)
- Layer 4: Model integrity (GGUF hash verification + SPoRA random-layer audits)
- Layer 3: Transport confidentiality (eavesdropping + per-ASN / max-connections caps)
- Layer 2: Local tampering (file modification)
- Layer 1: Network fraud (fake trade claims + replay protection + nonce-fraud proofs)
- Layer 0: Historical immutability (Merkle root on Base L2)
- Layer -1: Independent verification of the whole stack

## Phase 17 hardening components

Phase 17 introduced security primitives that cut across the
two-layer economic/inference split and form their own security
plane. See [`security/phase-17-summary.md`](security/phase-17-summary.md)
for the condensed audit-facing view, [`security/threat-model-v2.md`](security/threat-model-v2.md)
for the threat-to-code mapping, and the [roadmap](roadmap.md)
Phase 17 section for the per-wave highlights.

### Replay & integrity

- `tirami-ledger::TradeRecord.nonce: [u8; 16]` — provider-chosen
  replay nonce. v1 (zero-nonce) trades still work; v2 trades
  carry a 0x02 version-prefixed canonical layout.
- `tirami-ledger::ledger::NonceCache` — per-provider FIFO dedup
  (10 000 cap), rebuilt from `trade_log` on restart.
- `ComputeLedger::execute_signed_trade` — the only entry point
  for peer-originated trades; verifies both signatures AND checks
  the replay cache.

### Slashing pipeline (previously dead code)

- `tirami-ledger::SlashEvent` — persisted audit-trail entry.
- `ComputeLedger::update_trust_penalties` — runs
  `CollusionDetector` + `apply_slash`; 5-minute cooldown per node.
- `ComputeLedger::record_audit_failure_slash` — bridge from
  `AuditVerdict::Failed` (30% "major" tier penalty).
- `TiramiNode::spawn_slashing_loop` — every 5 min (clamped ≥ 60 s).

### Audit protocol extensions

- `AuditChallengeMsg.layer_index: Option<u32>` — SPoRA random-layer
  query. `None` = final output logits.
- `AuditTracker::resolve_at_layer` — normalizes `None ↔ Some(MAX)`;
  layer mismatch → `AuditVerdict::Unknown`.
- `audit_snark::ValidatorQuorum` — 2/3 majority tally for heavy
  audits.

### Sybil defense

- `tirami-net::asn_rate_limit::AsnRateLimiter` — per-ASN token
  bucket (5 000 msg/s default). Wired into the transport accept
  loop via `ForgeTransport::install_asn_limiter`.
- `tirami-ledger::sybil::WelcomeLoanLimiter` — per-bucket rolling
  24 h window. Wired into `can_issue_welcome_loan_limited`.
- `ForgeTransport.max_connections` — accept-time cap (default
  1 000). Over-cap handshakes dropped before `connecting.await`.

### State bounding

- `tirami-ledger::checkpoint::LedgerCheckpoint` — Merkle root of
  a sealed trade range.
- `ComputeLedger::seal_and_archive` — partitions `trade_log`,
  appends sealed slice to a JSON-lines archive.
- `TiramiNode::spawn_checkpoint_loop` — every 1 h by default.
- `tirami-ledger::PeerRegistry` — LRU cache, default cap 10 000.

### Divergence detection

- `tirami-ledger::fork::ForkDetector` — emits `Converged`,
  `InMinority`, or `NoQuorum` verdicts from peer Merkle-root
  observations.
- `tirami-ledger::fork::NonceFraudProof` — broadcastable
  double-sign evidence.
- `detect_nonce_conflict` — batch scanner.

### Authentication

- `tirami-node::api_tokens::ApiScope { ReadOnly, Inference,
  Economy, Admin }` + `TokenStore` (hash-only persistence,
  instant revocation). Legacy bearer honored as implicit Admin.
- `api::require_admin_scope` — helper for privileged handlers.

### Cryptography preparation

- `tirami-core::crypto::HybridSignature` — Ed25519 + optional
  ML-DSA. Both-or-fail verification when PQ half is present.
- `tirami-core::key_rotation::NodeIdentity` — epoch-based
  identity; `verify_historical` accepts revoked-key signatures
  produced before revocation.
- `tirami-core::attestation::AttestationReport` — hardware
  root-of-trust claim.

## Crate Dependencies

```
tirami-core ← shared types (NodeId, TRM, Config, HybridSignature, NodeIdentity, AttestationReport)
    ↑
tirami-ledger ← economic engine (trades, pricing, yield, slashing, checkpoints, fork, sybil)
    ↑
tirami-lightning ← external bridge (LDK wallet, TRM↔sats)
    ↑
tirami-anchor ← Base L2 on-chain anchor (MockChainClient + BaseClient scaffold)
    ↑
tirami-node ← orchestrator (HTTP API, pipeline, scoped API tokens, ledger integration)
    ↑
tirami-cli ← reference CLI (chat, seed, worker, settle)

tirami-net ← P2P transport (iroh, QUIC, Noise, mDNS, AsnRateLimiter)
tirami-proto ← wire messages (bincode, 14+ payload types incl. AuditChallenge/Response)
tirami-infer ← inference engine (llama.cpp, GGUF loader, SPoRA generate_audit_at_layer)
tirami-shard ← topology planner (layer assignment, rebalancing)
```
