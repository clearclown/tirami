# Tirami: A Compute-Backed Protocol for AI-Agent Economies

**Version 1.0 (Phase 18 draft)** · 2026-04-18
**Status:** Draft for external audit. Subject to review.

## Abstract

We describe Tirami, a distributed protocol in which unused GPU
compute performs verifiable inference work for AI agents in
exchange for TRM — a compute-backed token with a 21 billion
hard cap and Constitutional immutability. Unlike earlier
decentralized-compute protocols, Tirami couples work to
verification via zero-knowledge proofs of inference (zkML),
giving O(log n) verification cost for O(n) useful work. This
asymmetry — which Bitcoin achieved via SHA-256 and we achieve
via zkML — is the prerequisite for a currency whose value does
not rest on trust in a single operator. We present the
protocol, the Constitutional rule set, the zkML rollout ratchet,
and the bootstrap model.

## 1. Problem

Two problems converge in 2026:

1. **Centralized AI is a choke point.** Four companies control
   LLM inference. They set prices, content policies, retention
   terms, and access rules. No appeal.

2. **Distributed compute is economically inert.** Akash, io.net,
   Gensyn, Render, Bittensor, and dozens of smaller networks
   offer "earn money for GPU cycles" but none has reached
   protocol-grade credibility. They are SaaS companies with
   governance tokens, not currencies.

Both problems have the same root: no protocol has solved the
work-verification asymmetry at scale. Without it, participants
must trust someone — the operator, the staking committee, the
reputation algorithm. Bitcoin eliminated that trust for currency.
We propose to do the same for AI compute.

## 2. Core principle

**1 TRM = 10⁹ FLOP of verified useful work.**

Useful work means: a specified LLM, with a specified weight hash,
ran a specified prompt to produce a specified output. The proof
that this happened is cryptographic (zkML, Phase 21 target) and
verifiable without re-running the inference.

Consequences:
- Supply is bounded by compute done, not by mining decisions.
- Inflation is bounded by how fast the world's GPUs can do
  proof-backed work.
- The 21 B cap is a scarcity claim grounded in physical limit,
  not arbitrary.

Constitutional: see §6.

## 3. Participants

Four roles, no central operator:

| Role | Stakes | Earns |
|------|--------|-------|
| **Provider** | GPU, electricity, stake | TRM per verified inference |
| **Consumer** | TRM | Verified inference |
| **Validator** | Stake + uptime | Audit reward fraction |
| **Operator** | None (open source) | Nothing — the protocol is not a company |

Every role is permissionless. Providers stake TRM to activate; if
they have none, they earn up to 10 TRM from the bootstrap faucet
(once per identity, forfeited on slash).

## 4. Economic layer

### 4.1 Trades

A trade is a signed attestation:

```
TradeRecord {
    provider: NodeId,         // Ed25519 public key
    consumer: NodeId,
    trm_amount: u64,
    tokens_processed: u64,
    flops_estimated: u64,     // 10^9 × trm_amount (Principle 1)
    model_id: String,
    timestamp: u64,
    nonce: [u8; 16],          // 128-bit replay protection
}
```

Both parties sign the canonical byte form. Unsigned trades are
rejected (Phase 17 Wave 1.2). Replays are rejected via a
per-provider nonce cache. Gossip propagates signed trades; any
node can verify both signatures independently.

### 4.2 Slashing

Collusion detector identifies tight-cluster, volume-spike, and
round-robin patterns. Nodes exceeding a trust-penalty threshold
are slashed from their stake (5% / 20% / 50% tiers;
Constitutional). Audit failures apply the 20% tier directly.

### 4.3 Staking

Providers lock TRM for 7 / 30 / 90 days to activate. Minimum
stake is Constitutional (`MIN_PROVIDER_STAKE_CONSTITUTIONAL_FLOOR
= 10 TRM`); effective minimum is mutable above the floor (default
100 TRM). Stake earns yield proportional to reputation and
duration.

### 4.4 Market pricing

Each node computes its local view of supply/demand and applies a
deflation curve: `1 CU buys more tokens as the network matures`.
Gossip-weighted reputation influences effective price across
peers.

## 5. Verification layer

### 5.1 Signed trades (live, Phase 17)

Every paid trade has two Ed25519 signatures over canonical bytes.
Any third party can verify independently. Nonce prevents replay.

### 5.2 Challenge-response audit (live, Phase 14.3 + SPoRA)

A random challenger asks a random provider to hash a random
layer's activations for a shared input. Truncated-model
attackers cannot answer (SPoRA principle).

### 5.3 Heavy audit (scaffold, Phase 17 Wave 2.2)

1% of trades trigger a 3-validator quorum re-run. 2-of-3
majority decides. Dissenters are slashed.

### 5.4 zkML (rollout path, Phase 21)

Rollout ladder:
- `Disabled` (today) — trust-based, no proof.
- `Optional` (Phase 19) — proofs verified when present.
- `Recommended` (Phase 20) — no-proof trades reputation-capped.
- `Required` (Phase 21, Constitutional) — proofs mandatory.

Once `Required` is reached, the policy cannot be downgraded.
Constitutional ratchet enforced by `try_ratchet_proof_policy`.

## 6. The Constitution

Governance has a **closed whitelist** of mutable parameters.
Everything else is Constitutional and unchangeable without a
protocol fork.

### Immutable (Constitutional)

- `TOTAL_TRM_SUPPLY = 21 × 10⁹`
- `FLOPS_PER_CU = 10⁹`
- Halving curve (50% / 75% / 87.5% / ...)
- Slash rates (5% / 20% / 50%)
- Dual-signature + nonce requirements
- Canonical byte format v1/v2
- Proof-policy no-downgrade ratchet
- Welcome loan sunset epoch (2)

### Mutable (operational tuning)

- Welcome loan amount, LTV ratio, reserve ratio, etc.
- Market pricing tiers
- Rate limits (per-ASN, max connections)
- Audit sample rates
- Stake bonus multipliers
- Anchor / checkpoint intervals
- `MIN_PROVIDER_STAKE_TRM` (above Constitutional floor)
- `PROOF_POLICY` (ratcheted upward only)

Full list: `docs/constitution.md`. Enforcement lives in
`crates/tirami-ledger/src/governance.rs`.

## 7. Anti-Sybil

### 7.1 Stake-required mining

Providers need ≥ `MIN_PROVIDER_STAKE_TRM` to earn paid trades.
No-stake nodes access the 10 TRM bootstrap faucet (once per
identity). Slashed nodes forfeit the faucet.

### 7.2 Welcome loan sunset

1 000 TRM at 0% interest, 72 h term — available ONLY until
halving epoch 2. After that, new entrants must stake or use the
faucet.

### 7.3 Per-ASN rate limits

`AsnRateLimiter` caps inbound messages at 5 000/s per ASN.
Cloud-Sybil that shares one ASN shares one bucket.

### 7.4 Per-bucket welcome-loan cap

`WelcomeLoanLimiter` caps grants at 10 per ASN per 24 h. Stake-
proven peers get 10×.

## 8. Transport & storage

### 8.1 P2P transport

iroh QUIC + Noise. Each connection has per-peer 500 msg/s and
per-ASN 5 000 msg/s buckets. Max concurrent connections capped
(default 1 000, operator-tunable).

### 8.2 Ledger storage

JSON-lines snapshot with HMAC-SHA256 integrity. Trade log sealed
hourly (default) into a JSON-lines archive; in-memory retention
is bounded to 24 h (default). Each seal records a Merkle root
and a timestamp.

### 8.3 On-chain anchor

Every 10 minutes (configurable) the Merkle root of the
just-sealed range is submitted to Base L2 via
`TiramiBridge::storeBatch`. This is the auditable long-term
history.

## 9. On-chain contracts

### 9.1 TRM ERC-20

- Name: "Tirami Resource Merit" / Symbol: "TRM" / Decimals: 18.
- Supply cap: 21 × 10⁹ × 10¹⁸ wei. Enforced at mint.
- Mintable only by `TiramiBridge`; non-transferable otherwise.
- Burnable by any holder.

### 9.2 TiramiBridge

- `storeBatch(merkleRoot, batchId, nodeId)` — records an off-chain
  batch root. Idempotent by `batchId`.
- `mintForProvider(nodeId, to, flops)` — Principle-1 mint
  (`flops × 10⁹ × 10⁻⁹ = 1 TRM per 10⁹ FLOP`). Cooldown: 10 min.
- `deposit` / `requestWithdrawal` / `claimWithdrawal` — bridge
  flow with 60-minute withdrawal delay.
- Pausable by owner; mint-cooldown first-mint carve-out.

## 10. Security model

Layered defense. Higher-numbered layers require the lower ones.

| Layer | Defense |
|-------|---------|
| -1 | External professional audit (gated, pre-mainnet) |
| 0 | On-chain Merkle anchor (Base L2) |
| 1 | Dual signatures + nonce |
| 2 | Local HMAC-SHA256 ledger integrity |
| 3 | iroh QUIC + Noise transport |
| 4 | Local inference execution + SPoRA audits |
| 5 | Hardware attestation (optional premium) |

Wave-by-wave threat coverage: see
`docs/security/threat-model-v2.md`. 27 threats tracked; residual
risks documented in `docs/security/known-issues.md`.

## 11. Bootstrap & incentives

### Day 0 — Genesis

- 0 TRM exist. Welcome loan issues 1 000 TRM at 0 % interest to
  new nodes, capped by the per-ASN limiter. 72-hour term.
- Stake-less provider can earn up to 10 TRM via the bootstrap
  faucet.
- Providers run inference, earn TRM per 10⁹ FLOP of verified
  work, stake to activate.

### Epoch 0 → 1 — Growth

- 50 % of supply minted by end of epoch 0. Halving curve reduces
  issuance rate by 50 % per epoch.
- Audit cadence increases as network matures.
- Governance can tune operational parameters.

### Epoch 2 — Sunset

- Welcome loan closes permanently (Constitutional).
- New entrants use the faucet OR buy TRM off-protocol (DEX via
  Base bridge).
- Mainnet deploy is gated on external audit completion at this
  point.

### Epoch 3 → ∞ — Steady state

- 87.5 % → 100 % of TRM supply exists.
- Issuance rate continues halving.
- Yield for providers comes primarily from transaction fees
  (2 % of trade amount, to be introduced in Phase 19).

## 12. Bitcoin / Filecoin comparison

| Property | Bitcoin | Filecoin | Tirami |
|----------|---------|----------|--------|
| Useful work | SHA-256 (none) | Storage | Inference |
| Verification cost | O(1) | O(log n) via PoRep | O(log n) via zkML (Phase 21 target) |
| Supply cap | 21 × 10⁶ BTC | uncapped | 21 × 10⁹ TRM |
| Immutability | Mathematical | Governance | Constitutional whitelist |
| Miners stake? | No | Yes | Yes |
| Proof required? | Yes (PoW) | Yes (PoRep) | Yes (Phase 21) |
| Credible neutrality | High | Moderate | Moderate (ratcheting) |

We target **Filecoin-scale credibility** (~\$1-10 B market cap,
real independent infrastructure). Bitcoin-scale ($1 T+,
rule-immutability-by-math) is a 5-10 year project and depends on
zkML completion.

## 13. Open questions

1. **Real zkML performance** — current ezkl proves a 500 M
   parameter model in ~30 s. Required for `Optional` policy: ≤ 5×
   inference cost. Required for `Required`: ≤ 2×. Timeline:
   2-5 years.
2. **Two-sided market bootstrap** — how does a new user find
   their first AI agent, and how does that agent find the first
   provider? Research ongoing (see `docs/bootstrap.md`).
3. **Killer app** — the "AI agents paying for compute" market
   does not yet exist at scale. We are betting it will, and
   building the rails for when it does.

## 14. Non-goals

- We are NOT building a replacement for centralized LLM APIs
  today. We are building the infrastructure for when those APIs
  become insufficient (regulatory, economic, or political
  pressure).
- We are NOT aiming for Bitcoin-scale in 2026. Filecoin-scale
  credibility is the Phase 18-21 target.
- We are NOT issuing a governance token separate from TRM. TRM
  is the one asset.
- We are NOT doing an ICO. Early TRM is earned, not sold.

## 15. References

- Tirami code: `github.com/clearclown/tirami`
- Contracts: `repos/tirami-contracts/` (Foundry)
- Economic theory: `github.com/clearclown/tirami-economics`
- Constitution: `docs/constitution.md`
- Threat model: `docs/security/threat-model-v2.md`
- Audit scope: `docs/security/audit-scope.md`
- zkML strategy: `docs/zkml-strategy.md`
- Public API surface: `docs/public-api-surface.md`

## 16. Acknowledgments

Built on `mesh-llm` by Michael Neale (distributed inference
layer). Inspired by Bitcoin (proof of work), Filecoin
(proof of storage), Worldcoin (production zkML). Standing on
the shoulders of specific giants.
