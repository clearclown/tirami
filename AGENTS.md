# AGENTS.md — Tirami

> Guidance for AI coding agents working in this repo. For the full
> development guide see [`CLAUDE.md`](CLAUDE.md); for the authoritative
> functional-today / scaffolded / not-done breakdown see the README
> "⚠️ Status Honesty" section. This file is the condensed orientation.

## Project

Tirami is a distributed LLM inference protocol where **compute is currency**.
The inference layer is built on [mesh-llm](https://github.com/michaelneale/mesh-llm);
Tirami's original contribution is the **economic layer** — TRM (Tirami
Resource Merit) accounting, Proof of Useful Work, dynamic pricing, and
autonomous agent budgets. TRM is compute accounting (1 TRM = 10⁹ FLOP),
**not a financial product** — no ICO, no pre-mine, no airdrop.

**Tagline:** "Computation is currency. Every watt produces intelligence, not waste."

## Workspace Structure

Rust monorepo (edition 2024, resolver v2), **16 crates**:

```
crates/
  tirami-core/      — Shared types (NodeId, TRM, Config, PriceSignal, AuditTier, InferenceTicket, attestation scaffold)
  tirami-net/       — P2P networking via iroh (QUIC, Noise, NAT traversal, gossip)
  tirami-shard/     — Model layer partitioning / topology
  tirami-infer/     — Inference engine (llama.cpp + GGUF + Metal/CPU, generate_metered/audit)
  tirami-proto/     — Wire protocol message types (serde + bincode; 30+ types incl. audit challenge/response)
  tirami-ledger/    — Core economic engine (TRM, trades, lending, tokenomics, staking,
                       governance, collusion, slashing, PeerRegistry, select_provider, audit)
  tirami-node/      — Node daemon, HTTP API (60+ endpoints), pipeline, persistent wallet, background loops
  tirami-cli/       — Reference CLI (chat, seed, worker, start, settle, wallet, su, agent)
  tirami-lightning/ — TRM <-> Bitcoin Lightning bridge (bidirectional)
  tirami-bank/      — L2: strategies, portfolios, futures, insurance, risk, yield
  tirami-mind/      — L3: AutoAgent self-improvement loops paid in TRM
  tirami-agora/     — L4: agent marketplace, reputation, NIP-90
  tirami-sdk/       — Rust async HTTP client for the Tirami API
  tirami-mcp/       — Rust MCP server (44 tools for Claude/Cursor/ChatGPT)
  tirami-anchor/    — Periodic Merkle-root anchor loop + swappable ChainClient (Phase 16)
  tirami-zkml-bench/— zkML proof-policy ratchet + benchmark harness (MockBackend; ezkl/risc0 pending)
```

(`crates/tirami-zkml-bench-guest` is a risc0 guest, not a workspace member.)

## Build & Test

```bash
cargo build --release      # Full build
cargo test --workspace     # 1,574 tests across 16 crates
cargo check --workspace    # Fast type check
cargo clippy --workspace   # Lint
bash scripts/verify-impl.sh  # TDD conformance check
```

## Key Design Rules

- **CU/TRM is the native currency.** Bitcoin/Lightning is an optional
  off-ramp, never a hard dependency in the economic engine.
- **Trades and loans are bilateral.** Every transfer is dual-signed
  (Ed25519) by provider + consumer (or lender + borrower) and gossiped,
  with 128-bit nonce replay protection.
- **No global consensus.** TRM accounting uses local ledgers + gossip +
  dual signatures. On-chain anchoring (Base L2, Bitcoin OP_RETURN) is an
  optional audit layer, not the source of truth.
- **No tokens, no ICO.** TRM is earned by useful computation, not sold
  (see SECURITY.md § Secondary Markets).
- **Agent-first API.** `/v1/tirami/*` endpoints exist so AI agents can make
  autonomous economic decisions without human help.
- **Fail-safe.** If any safety check cannot determine safety, it denies.
- **Governance is constitutional.** 18 immutable constitutional parameters
  vs 21 mutable governance parameters; proposals outside the mutable list
  auto-reject. Don't widen the mutable set without spec change.

## What NOT to Do

- Do NOT make Bitcoin/Lightning a hard dependency of the economic core.
- Do NOT add unilateral trades or loans — both parties must sign.
- Do NOT send unencrypted data over the network (Noise over QUIC).
- Do NOT re-define economic constants in Rust — reference the theory
  spec (`parameters.md`) as the single source of truth.
- Do NOT execute external payments in the protocol core — settlement
  endpoints export data; bridges are adapters outside the core.
- Do NOT overstate status in docs — match the README "Status Honesty"
  section (functional-today vs scaffolded vs not-done).

## Conventions

- Errors: `TiramiError` enum in `tirami-core`; `anyhow` in the CLI only.
- Serialization: `serde` for JSON/config, `bincode` for the wire protocol.
- Async: `tokio`, `Arc<Mutex<T>>` for shared state.
- Logging: `tracing` — INFO for user-visible events, DEBUG for protocol detail.
- Security: HMAC-SHA256 for ledger integrity, Ed25519 for trade/loan
  signatures (persistent node wallet at `~/.tirami/node.key`), Noise for
  transport, constant-time auth-token comparison.

## Docs

- `CLAUDE.md` — full development guide (crate map, API surface, common tasks)
- `README.md` — Status Honesty (authoritative implemented/scaffolded/not-done)
- `docs/economy.md` — Compute Standard, TRM, trades, yield, lending
- `docs/architecture.md` — two-layer (economic / inference) design
- `docs/protocol-spec.md` — wire protocol specification
- `docs/roadmap.md` — implementation phases
- `docs/threat-model.md` + `docs/security/` — security + economic threats
- `docs/release-readiness.md` — mainnet audit gates
