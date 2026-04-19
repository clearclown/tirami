# Tirami — Release Readiness (2026-04-19)

A concise, honest assessment of "can we publish this now?"
Updated after the Phase 18.5-part-3 E2E fix wave closed all
open issues (#73–#85) and a live 2-node verification on
`100.112.10.128` confirmed end-to-end dual-signed TRM
negotiation for the first time via HTTP.

## Verdict by scale tier

| Tier | Audience / scope | Verdict | Notes |
|---|---|---|---|
| **A — OSS public preview** | Repo public, tweet, blog, Hacker News; devs run `tirami start` locally | ✅ **READY** | MIT licensed, no real money, 1 185 passing tests, transparent SECURITY.md, placeholder PGP marked as such. |
| **B — Invited testnet** | ≤100 node operators, TRM stays virtual (no external value), you track uptime | ✅ **READY** with caveats below | Phase 18.5-part-3 closed every blocker identified on the 2026-04-18 E2E run. |
| **C — Open public testnet** | 1 000+ nodes, open registration, still virtual TRM | ⚠️ **NOT YET** | Needs (1) peer auto-discovery so callers don't hand-wire `peer.url`, (2) `ProofPolicy` raised to `Optional` at minimum, (3) ≥ 7-day stress test at 10+ nodes, (4) bug-bounty live with real PGP. |
| **D — Mainnet with real value** | Base L2 TRM ERC-20, real capital | ❌ **FORBIDDEN by own plan** | Phase 17 Wave 3.3 explicitly gates this on external security audit completion. |

## What's ready now (Tier A + B)

### Core protocol
- **Dual-signed TRM trades over HTTP.** Live-verified 2-node: HTTP client → worker `/v1/chat/completions` → iroh P2P → seed `handle_inference` → TradeProposal → worker counter-sign → TradeAccept → seed `execute_signed_trade` → gossip. Consumer = real NodeId, trm = full amount, not penalised. Seed log: `Signed trade recorded: 24 CU for 16 tokens to …`.
- **9 unit tests** on signature verification alone (replay, bit-flip, crossed sigs, wrong sigs, real keys).
- **Nonce-based replay protection** (Phase 17 Wave 1.2) — `execute_signed_trade` rejects nonce reuse per-provider.
- **Slashing** wired into the seed loop every `slashing_interval_secs` (default 300 s) with a `SlashEvent` audit trail at `/v1/tirami/slash-events`.
- **Stake-required mining** (Phase 18.2): providers need `MIN_PROVIDER_STAKE_TRM = 100` active stake OR be within the `STAKELESS_EARN_CAP_TRM = 10` bootstrap faucet. Slashed nodes forfeit the faucet.
- **Governance whitelist** (Phase 18.1): 21 mutable parameters, 18 Constitutional (immutable via governance) — TOTAL_TRM_SUPPLY, FLOPS_PER_CU, slash rates, signature invariants all locked.

### HTTP API hygiene
- **Consistent JSON error envelope** `{error:{code,message}}` on every 4xx/5xx — no more plaintext leaks.
- **Auto-configured PersonalAgent** on `tirami start` → `/v1/tirami/agent/*` reachable.
- **Scoped API tokens** via `/v1/tirami/tokens/issue` with `node_id` auto-defaulting.
- **Prometheus `/metrics`** using `tirami_*` prefix, anonymous sentinel filtered out, pricing rounded to 6 dp.
- **Rate-limited** economic endpoints (30 req/s token bucket).
- **DDoS cap** `max_concurrent_connections` (default 1 000) on the QUIC transport.
- **Tracing filter** silences iroh/mDNS noise for default `RUST_LOG`.

### Observability
- 13 Prometheus metrics (cu_contributed, cu_consumed, reputation, trade_count, active_loans, pool_total_trm, pool_reserve_ratio, collusion_*, governance_*, tokenomics_*).
- `loop.ticks` / `last_action` / `last_tick_ms` on `/v1/tirami/agent/status` so operators can see the agent loop is alive.

### Supply-chain / build
- Rust edition 2024, workspace v0.3.0, release binary 51 MB (aarch64-darwin).
- `cargo test --workspace` 1 185 passing, 0 failed.
- `cargo check --workspace` 3 cosmetic warnings (dead-code, legacy), 0 errors.
- MIT license, no secrets in tree.

## What is NOT ready (Tier C / D blockers)

### Peer discovery
- `POST /v1/tirami/agent/task` RunRemote branch requires the caller to hand-wire `peer.url`. `select_provider` is called but can't yield an HTTP address — `PriceSignal` has no URL field.
- **Fix requires** adding an HTTP-advertised address to `PriceSignal` + a resolver layer. Non-trivial protocol change; out-of-scope for testnet-B invited batch but needed for open testnet.

### zkML / proof-of-inference
- `ProofPolicy` default is `Disabled`. Lazy providers aren't cryptographically deterred — only by reputation + audit challenges.
- `tirami-zkml-bench` crate has scaffolding for ezkl / risc0 / halo2; real backends not wired.
- **Fix requires** promoting `ProofPolicy` to `Optional` with a working MockBackend (or ezkl for one model), then `Required` before mainnet.

### Post-quantum signatures
- `Config::pq_signatures = false` by default; ML-DSA hybrid is scaffolded but blocked on iroh 0.97 dep conflict with `digest 0.11.0-rc.10`.
- **Fix requires** iroh dep chain to settle, OR a fork that decouples `ml-dsa` from the shared digest version.

### TEE attestation
- `tirami-attestation` scaffold exists (Apple SE / NVIDIA H100 CC placeholders). No real attestation on production provider nodes today.

### External security audit
- Not started. Phase 17 Wave 3.3 docs ready (`docs/security/audit-scope.md`, `threat-model-v2.md`, `known-issues.md`), candidates listed (Trail of Bits, Zellic, Open Zeppelin, Least Authority).

### Long-running stability
- No ≥ 7-day testnet run of 10+ nodes.
- No ≥ 30-day Sepolia contracts deployment.
- `tirami-contracts` has 15 Foundry tests passing but hasn't been deployed to Base Sepolia from this branch.

### Bug bounty
- SECURITY.md framework drafted; **active payouts NOT live**. PGP block is a placeholder (self-documented).

## Recommended release sequence

1. **Today — publish Tier A.** OSS public preview. Repo already public; nothing new required. Post an HN/Twitter announcement linking to this file + the whitepaper.
2. **This week — open Tier B.** Invite ≤100 operators (DMs / community call). Each runs `tirami start` or `tirami worker --seed <hex>`. Monitor `/metrics` + collect any new issues. No economic risk because TRM has no external value.
3. **Weeks 2–4 — Tier B stress.** Run 10+ operator-hosted nodes for ≥ 7 days. Measure: signed-trade rate, gossip convergence, slashing triggers, log noise, 95th-percentile HTTP latency, memory / disk growth. File + fix issues. Bump `ProofPolicy` from Disabled → Optional once MockBackend roundtrip is reproducible.
4. **Month 2 — external audit kickoff.** Scope already documented. Engage 1–2 auditors from the candidate list. Freeze feature work on audit-scope crates during the review (`tirami-core`, `tirami-ledger`, `tirami-node`, `tirami-contracts`).
5. **After audit — Tier C.** Public testnet with `ProofPolicy = Recommended`, live bug bounty, real PGP key, Sepolia contracts deployed ≥ 30 days.
6. **After bug bounty closes a clean quarter — Tier D.** Mainnet deployment of TRM ERC-20 on Base L2. Ratchet `ProofPolicy = Required` Constitutionally (irreversible).

## Checklist for the Tier A / Tier B announcement

- [x] `cargo test --workspace` green (1 185/0)
- [x] All 6 E2E-surfaced issues (#80–#85) closed
- [x] All 6 previously-surfaced issues (#73–#78) closed
- [x] SECURITY.md present + honest (PGP placeholder marked)
- [x] LICENSE (MIT)
- [x] README badges current (1 185 tests, Phase 18.5)
- [x] CHANGELOG [Unreleased] covers Phase 18.5-part-3
- [x] 2-node TRM negotiation verified live
- [ ] Blog post / HN submission text drafted — **follow-up**
- [ ] Demo video / GIF — **follow-up**
- [ ] Operator quick-start (`docs/operator-guide.md`) reviewed for Phase 18.5 changes — **follow-up**

## Bottom line

**Can you publish Tirami today?** Yes — as an open-source preview (Tier A) and as an invited testnet (Tier B). The 2-node E2E ran a real dual-signed TRM trade over HTTP for the first time; the Phase 18.5-part-3 fix wave closed every blocker that matters at this scale. Mainnet still waits for external audit per the Phase 17 plan.
