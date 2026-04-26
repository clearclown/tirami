# Tirami — Release Readiness (2026-04-27, public testnet prep)

A concise, honest assessment of "can we publish this now?"
Updated after the public-testnet preparation pass:

- Peer auto-discovery via `PriceSignal.http_endpoint` (#114).
- Agent remote dispatch can auto-select a provider from PriceSignal
  gossip and inherit the caller bearer for shared-token private
  testnets.
- Public bootstrap peer joins via `Config::bootstrap_peers`,
  `--bootstrap-peer`, and `TIRAMI_BOOTSTRAP_PEERS`.
- Public/wildcard HTTP binds now fail closed unless `--api-token` or
  `TIRAMI_API_TOKEN` is set.
- `ProofPolicy` default promoted `Disabled → Optional` (#115).
- Base Sepolia deployment Makefile + gated mainnet target (#116).
- Secondary-market + audit-gated disclaimer in `SECURITY.md`
  and `README.md` (#117).

Follow-up known gap — filed as #88 (P2, not a blocker): worker
`--daemon` has no gossip recv loop. Full `tirami start` seed-style
nodes do receive + ingest gossip, advertise HTTP endpoints when bound
to concrete reachable addresses, and can auto-dispatch agent remote
tasks without explicit `peer.url` hints.

The 2026-04-26 live 2-node Tailscale E2E on `100.112.10.128`
and `100.107.30.86` confirmed agent remote dispatch without an
explicit peer hint: ASUS selected the Mac Studio provider from
PriceSignal gossip, forwarded the bearer token, recorded the same
provider/consumer trade on both ledgers, and restored it after
restart. After two remote jobs, Mac agent `earned_today_trm=18`,
ASUS agent `spent_today_trm=18`, and both ledgers reported
`total_trades=2`.

## Verdict by scale tier

| Tier | Audience / scope | Verdict | Notes |
|---|---|---|---|
| **A — OSS public preview** | Repo public, tweet, blog, Hacker News; devs run `tirami start` locally | ✅ **READY** | MIT licensed, no real money, workspace tests green, transparent SECURITY.md, placeholder PGP marked as such. |
| **B — Invited testnet** | ≤100 node operators, TRM stays virtual (no external value), you track uptime | ✅ **READY** with caveats below | 2-node agent remote spend/earn is live-verified over Tailscale with persisted ledgers. |
| **C — Open public testnet** | 1 000+ nodes, open registration, still virtual TRM | 🟡 **Bootstrap plumbing READY, operational blockers remain** | Public join strings are supported (`PUBLIC_KEY@RELAY_URL` and `PUBLIC_KEY@IP:PORT`). Still pending: ≥ 7-day stress at 10+ nodes, published seed list/status page, bug bounty live with real PGP, worker daemon gossip loop (#88). |
| **D — Mainnet with real value** | Base L2 TRM ERC-20, real capital | 🟡 **Infrastructure READY, audit gate active** | Sepolia deploy Makefile + mainnet target gated on `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER` + interactive confirmation (Phase 19). Mainnet deploy still blocked on external audit. Secondary-market disclaimer landed in SECURITY.md / README / deployments/README. |

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
- **Agent remote auto-dispatch** from advertised peer HTTP endpoints on
  full nodes. Shared-token private labs can omit `peer.url`; the local
  bearer is forwarded to the selected provider.
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
- `cargo test --workspace` green on the current release-prep branch.
- `cargo check --workspace` 3 cosmetic warnings (dead-code, legacy), 0 errors.
- MIT license, no secrets in tree.

## What is NOT ready (Tier C / D blockers)

### Open-testnet operations
- Public bootstrap joins are wired, but the project has not yet run a
  7-day, 10+ node public mesh using the new bootstrap list.
- No published canonical seed list or public status page exists yet.
- `worker --daemon` still has no gossip recv loop (#88), so some
  flows need a full `tirami start` node or an explicit peer hint.

### zkML / proof-of-inference
- `ProofPolicy` default is `Optional`. Lazy providers are not yet
  cryptographically deterred — only by reputation + audit challenges.
- `tirami-zkml-bench` crate has scaffolding for ezkl / risc0 / halo2; real backends not wired.
- **Fix requires** a real proof backend for at least one model, then
  promoting `ProofPolicy` to `Required` before mainnet.

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
3. **Weeks 2–4 — Ring 0/1 public-testnet prep.** Follow `docs/public-testnet-launch.md`: run 2-3 bootstrap seeds, then 10+ operator-hosted nodes for ≥ 7 days. Measure: signed-trade rate, gossip convergence, slashing triggers, log noise, 95th-percentile HTTP latency, memory / disk growth. File + fix issues.
4. **Month 2 — external audit kickoff.** Scope already documented. Engage 1–2 auditors from the candidate list. Freeze feature work on audit-scope crates during the review (`tirami-core`, `tirami-ledger`, `tirami-node`, `tirami-contracts`).
5. **After audit — Tier C.** Public testnet with `ProofPolicy = Recommended`, live bug bounty, real PGP key, Sepolia contracts deployed ≥ 30 days.
6. **After bug bounty closes a clean quarter — Tier D.** Mainnet deployment of TRM ERC-20 on Base L2. Ratchet `ProofPolicy = Required` Constitutionally (irreversible).

## Checklist for the Tier A / Tier B announcement

- [x] Targeted Rust tests green for the touched surface:
      `cargo test -p tirami-node` (208) and
      `cargo test -p tirami-cli` (6)
- [x] All 6 E2E-surfaced issues (#80–#85) closed
- [x] All 6 previously-surfaced issues (#73–#78) closed
- [x] SECURITY.md present + honest (PGP placeholder marked)
- [x] LICENSE (MIT)
- [x] README status current for 2026-04-27 / Phase 19
- [x] CHANGELOG [Unreleased] covers the 2026-04-26 private-lab result
- [x] 2-node remote-agent TRM spend/earn verified live
- [ ] Blog post / HN submission text drafted — **follow-up**
- [ ] Demo video / GIF — **follow-up**
- [x] Operator quick-start includes public bootstrap peers and API-token public-bind guard
- [x] Public testnet runbook added (`docs/public-testnet-launch.md`)

## Bottom line

**Can you publish Tirami today?** Yes — as an open-source preview
(Tier A) and as an invited private testnet (Tier B). The current
2-node E2E proves remote PersonalAgent spend/earn across two real
machines, with matching provider/consumer ledger records restored
after restart. Open public testnet still waits for a canonical seed
list, status page, bug-bounty/PGP readiness, and a 10+ node 7-day run.
Mainnet still waits for external audit per the Phase 17 plan.
