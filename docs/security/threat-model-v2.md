# Tirami Threat Model — v2 (Post-Phase-17)

**Superseded document:** `docs/threat-model.md` (the original v1).
Keep the v1 for historical reference; consult this v2 for the current
state of every threat.

This v2 rolls up the Phase 17 Delta appendix and re-scores every
threat with its current residual risk as of 2026-04-18. Format:

- **ID** — stable identifier.
- **Threat** — one-line description.
- **Severity** — pre-mitigation worst case (Critical / High / Medium / Low).
- **Residual** — remaining risk after current mitigations (Closed /
  Low / Medium / High).
- **Mitigation** — the current defense.
- **Wave** — which Phase-17 wave landed the main mitigation.

## Economic-layer threats

### T1: Malicious seed node

- **Severity:** Critical.
- **Residual:** Low.
- **Mitigation:** Ed25519 dual-signed TradeRecords; nodes reject
  unsigned trades at the wire (Wave 1.2). Gossip propagation verifies
  both signatures before applying.
- **Wave:** 1.2.

### T3: Sybil attack on free tier / welcome loans

- **Severity:** High.
- **Residual:** Medium.
- **Mitigation:** Local "100 unknown nodes" cap, plus
  `WelcomeLoanLimiter` per-bucket rolling 24-hour window (default 10
  grants) with a 10× multiplier for stake-proven peers (Wave 2.8).
  Integration into `can_issue_welcome_loan` is Wave-2.8-part-2
  (see K-005); the existing cap still triggers.
- **Wave:** 2.3 (per-ASN) + 2.8 (welcome loans).

### T4: Byzantine inference (lazy / truncated-model provider)

- **Severity:** High.
- **Residual:** Medium.
- **Mitigation:** Audit challenge-response with SPoRA layer scope —
  the challenger picks a random intermediate layer, the target must
  hash its activations. A truncated model can't answer (Wave 2.1).
  Failed audits slash 30 % of stake (Wave 1.4). 1 % probabilistic
  heavy audit with 2/3-validator quorum (Wave 2.2 scaffold; wire-up
  deferred).
- **Wave:** 1.4 + 2.1 + 2.2.

### T10: TRM forgery via replay

- **Severity:** Critical.
- **Residual:** Low.
- **Mitigation:** `TradeRecord` carries a 128-bit provider nonce
  (Wave 1.1). `ComputeLedger::execute_signed_trade` rejects duplicate
  nonces via a per-provider FIFO cache (Wave 1.2), rebuilt from
  `trade_log` on restart.
- **Wave:** 1.1 + 1.2.

### T11: Free-tier abuse

- **Severity:** High.
- **Residual:** Low.
- **Mitigation:** Welcome-loan replaces the flat 1 000 TRM grant
  (Phase 5.5). Per-ASN rolling cap (Wave 2.8) blocks
  cloud-massed Sybil.
- **Wave:** 2.8.

### T12: Ledger divergence / fork

- **Severity:** High.
- **Residual:** Medium.
- **Mitigation:** `ForkDetector` collects peer Merkle roots; if the
  local root is in the minority, the operator is notified (Wave 2.5).
  `NonceFraudProof` catches double-signed nonces (Wave 2.5). Full
  automatic resync protocol is Wave-2.5-part-2 (K-006).
- **Wave:** 2.5.

### T13: Market manipulation

- **Severity:** Medium.
- **Residual:** Medium.
- **Mitigation:** Market price is computed locally; no single peer
  can dictate another's pricing. Reputation-weighted gossip of price
  signals dampens adversarial input (existing Phase 9 mechanism).
- **Wave:** pre-Phase-17.

### T14: Inference quality attack (re-listed under T4)

See T4.

### T15: Loan default cascading

- **Severity:** High.
- **Residual:** Low.
- **Mitigation:** 30 % reserve requirement, 3:1 max LTV, 20 % max
  single-loan-to-pool ratio, default-rate circuit breaker, and
  per-identity caps based on credit score (all pre-Phase-17).
- **Wave:** pre-Phase-17 (Phase 5.5).

### T16: Credit score manipulation

- **Severity:** Medium.
- **Residual:** Medium.
- **Mitigation:** Multi-factor credit score (trade volume +
  repayment + uptime + account age), minimum 7-day account age for
  borrowing, score decay when inactive. Collateral 3:1 caps worst
  case loss.
- **Wave:** pre-Phase-17 (Phase 5.5).

### T17: Lending pool depletion

- **Severity:** High.
- **Residual:** Low.
- **Mitigation:** 30 % reserve, 50 %-in-1-hour global lending
  circuit breaker, rate limit on new loans.
- **Wave:** pre-Phase-17 (Phase 5.5).

## Phase-17-introduced threats (Wave 3 addresses the rest)

### T18: Dead slashing code (prior internal finding)

- **Severity:** High.
- **Residual:** Closed.
- **Mitigation:** `update_trust_penalties` called every 5 minutes
  (Wave 1.3). `record_audit_failure_slash` called on every
  `AuditVerdict::Failed` (Wave 1.4). Persisted `SlashEvent` audit
  trail exposed via `/v1/tirami/slash-events`.
- **Wave:** 1.3 + 1.4.

### T19: Single-secret API bearer token

- **Severity:** High.
- **Residual:** Low.
- **Mitigation:** Scoped tokens (`ReadOnly / Inference / Economy /
  Admin`), per-endpoint scope gating via `require_admin_scope`,
  hash-only persistence, instant revocation (Wave 1.5). Legacy
  `api_bearer_token` is retained as implicit Admin for transition.
- **Wave:** 1.5.

### T20: Post-quantum cryptography horizon (CRQC)

- **Severity:** High (time-bound: mid-2030s).
- **Residual:** Medium (mitigation scaffolded).
- **Mitigation:** `HybridSignature` scaffold with Ed25519 + optional
  ML-DSA, `PqSigner` / `PqVerifier` traits (Wave 1.6). Real ML-DSA
  backend deferred pending dep-pin resolution (K-002).
- **Wave:** 1.6.

### T21: Unbounded PeerRegistry memory creep

- **Severity:** Medium.
- **Residual:** Closed.
- **Mitigation:** LRU cache with 10 000 cap, auto-eviction on
  insert past capacity (Wave 2.6).
- **Wave:** 2.6.

### T22: Unbounded trade-log growth

- **Severity:** Medium.
- **Residual:** Closed (primitive); Low (operator-action
  required to schedule seals).
- **Mitigation:** `seal_and_archive(cutoff)` partitions the log,
  writes sealed range to a JSONL archive, records a
  `LedgerCheckpoint` with Merkle root (Wave 2.4). The daemon
  scheduling loop is Wave-2.4-part-2.
- **Wave:** 2.4.

## Wave 3-addressed threats (this document tracks)

### T23: Node key compromise without rotation path

- **Severity:** High.
- **Residual:** Medium (scaffold only in this wave).
- **Mitigation:** `NodeIdentity` gains multiple `KeyEpoch` entries
  (Wave 3.5). Old keys can verify historical trades; new trades
  must use the current key.
- **Wave:** 3.5.

### T24: DDoS against seed node HTTP / P2P endpoints

- **Severity:** High.
- **Residual:** Medium.
- **Mitigation:** Configurable `max_concurrent_connections` limit,
  operator guide recommends Cloudflare / Caddy in front (Wave 3.4).
  iroh's QUIC internals handle SYN floods.
- **Wave:** 3.4.

### T25: TEE-less peer claims hardware-backed trust

- **Severity:** Medium.
- **Residual:** Medium.
- **Mitigation:** Optional TEE attestation (Apple Secure Enclave /
  NVIDIA H100 CC) — attested peers get an advertisement bit + audit
  tier promotion priority. Explicitly NOT required (Wave 3.1).
- **Wave:** 3.1.

### T26: No formal verification of economic invariants

- **Severity:** Medium.
- **Residual:** Medium.
- **Mitigation:** Kani `#[kani::proof]` invariants for TRM
  conservation, signature-required balance changes, slash-burns
  total_supply (Wave 3.2 initial set; 30+ before external audit).
- **Wave:** 3.2.

### T27: Responsible disclosure has no process

- **Severity:** Medium.
- **Residual:** Low.
- **Mitigation:** `SECURITY.md` at repo root with PGP key,
  bounty scale, 72-hour SLA, and hall-of-fame (Wave 3.6).
- **Wave:** 3.6.

## Residual threats (explicitly accepted)

### R-001: Provider-consumer collusion

A provider and consumer can collude to inject fake trades. The
economic incentive is zero (the consumer loses TRM they would
otherwise keep). Statistical anomaly detection via `CollusionDetector`
monitors for tight-cluster / round-robin patterns and applies
trust-penalty slashing (Wave 1.3).

### R-002: Individual dishonest miner honest inputs

A provider can return dishonest output to a consumer who has no
ability to verify. The audit layer (Phase 14.3 + Wave 2.1 SPoRA)
provides probabilistic detection, not deterministic. A sufficiently
lucky dishonest provider operating below the audit rate will
occasionally evade. This is inherent to any open-participation
compute marketplace without SNARK proofs; Wave 2.2 scaffold plus
Phase 18 SNARK compression narrows the window.

### R-003: iroh peer discovery manipulation

We trust iroh's relay layer for initial peer discovery. A malicious
relay can partition the network view of a victim. iroh's upstream
threat model applies; out of scope for this audit.

---

*This is a living document. Changes require a PR and a review from
anyone who knows what the economic model is doing.*
