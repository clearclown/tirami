# Known Security Issues — Tirami

**As of:** 2026-04-18, Phase 17 Wave 3 in progress.

Purpose of this document: disclose every issue we've identified but
not yet fully remediated, so auditors don't spend budget re-finding
them. When an issue is fully mitigated, it moves to `CHANGELOG.md`
and is removed from this list.

All items here have been internally triaged. Severity reflects our
own assessment; the external auditor is welcome to disagree.

## Resolved since last version

| ID | Title | Resolved in |
|----|-------|-------------|
| T10 | TRM replay via fixed `TradeRecord` tuple | Wave 1.1 + 1.2 |
| T18 | `apply_slash` was unreachable in production | Wave 1.3 + 1.4 |
| T19 | Single global API bearer token | Wave 1.5 |
| T6 | No PeerRegistry eviction | Wave 2.6 |
| T13.a | Per-peer rate limit bypass via cloud ASN | Wave 2.3 |
| T14 | Truncated-model audit bypass | Wave 2.1 (SPoRA) |
| T15 | Unbounded trade-log memory | Wave 2.4 + 4.3 |
| T16 | Silent-fork divergence without detection | Wave 2.5 |
| K-004 | Per-ASN limiter not wired into transport | Wave 4.4-live |
| K-005 | Welcome-loan limiter not wired into ledger | Wave 4.1 |
| K-006 | Daemon checkpoint loop missing | Wave 4.3 |

## Open — tracked for remediation

### K-001: Legacy v1 (zero-nonce) trades bypass replay dedup

**Severity:** Medium.

**Description:** `ComputeLedger::execute_signed_trade` intentionally
skips the nonce cache for any `TradeRecord` with
`nonce == [0; 16]`, preserving backward compatibility with
pre-Phase-17 peers. An attacker who can continue to generate v1
records can replay them without rejection.

**Why it's open:** removing v1 breaks federation with existing
clients. The plan is to flip `config.reject_legacy_v1 = true` once
all SDKs have been bumped. This is blocked on a 30-day soak time
after all first-party SDKs emit v2.

**Mitigation in the meantime:**
- V1 records are never gossip-amplified by Phase-17 nodes.
- Nodes can explicitly refuse v1 via a runtime setting.
- Wave 2.5 `NonceFraudProof` only fires on v2 collisions, so it
  can't be spoofed by v1 replay.

**Fix tracking:** Phase 18.1.

### K-002: Real ML-DSA (post-quantum) binding not yet wired

**Severity:** Medium (time-bound — when CRQC arrives).

**Description:** Wave 1.6 delivers the `HybridSignature` type
lattice and a mock PQ verifier. The real `ml-dsa` 0.1.0-rc.8 crate
pulls `digest 0.11.0`, which conflicts with iroh 0.97's
`digest 0.11.0-rc.10`. Without the real backend, `pq_sig` is a
SHA-256-based mock that's trivially forgeable.

**Why it's open:** waiting for ml-dsa → stable release, or for iroh
to bump its digest pin. Both are external dependencies.

**Mitigation:** `config.pq_signatures` defaults to `false`; the
scaffold is not advertised as cryptographically sound.

**Fix tracking:** Phase 17 Wave 1.6-part-2.

### K-003: BaseClient write path is scaffold-only

**Severity:** Not-applicable (feature gate not open).

**Description:** The `BaseClient` in `tirami-anchor` returns
`ChainError::NotImplemented` on `store_batch`. The same dep pin
that blocks ML-DSA blocks ethers-rs integration.

**Why it's open:** same root cause as K-002.

**Mitigation:** `MockChainClient` is the default; no production
code writes via `BaseClient` today.

**Fix tracking:** Phase 17 Wave 2.7-part-2, concurrent with K-002.

### K-004: Per-ASN rate limiter in transport — RESOLVED

**Severity:** Was Medium (Sybil defense gap); closed 2026-04-18.

**Description (historical):** Wave 2.3 shipped `AsnRateLimiter`
as a primitive but the integration with the iroh transport was
initially thought to be blocked on an absent `remote_address()`
accessor in iroh 0.97's public `Connection` API.

**Resolution (Phase 17 Wave 4.4-live):** iroh 0.97 DOES expose
the peer IP, just via a different path than initially searched:

* `iroh::endpoint::Connection::paths()` returns a `PathWatcher`
  which yields `PathInfo` per known path.
* `PathInfo::remote_addr()` returns `&TransportAddr`.
* `TransportAddr::Ip(SocketAddr)` pattern-matches to expose the
  raw IP.

`PeerConnection::remote_ip()` now walks this path and returns
`Option<IpAddr>`; `None` for relay-only peers. The transport's
`start_accepting` loop:

1. Awaits the handshake so `remote_ip()` is populated.
2. Calls `AsnRateLimiter::take(ip)` on the installed limiter.
3. Drops the `PeerConnection` on over-cap, incrementing the
   `dropped_asn_over_cap` counter.

The limiter is installable via `ForgeTransport::install_asn_limiter`;
if never installed, the ASN check is skipped entirely (preserving
backward-compat for operators without a MaxMind DB).

**Mitigation level now:** strong. A cloud-Sybil attacker using
100 AWS IPs inside ASN 16509 can no longer collect 100× the
per-peer quota; all 100 share a single 5 000 msg/s bucket keyed
by the ASN resolved from their shared `PathInfo::remote_addr`.

### K-005: Welcome-loan limiter not yet wired into ledger

**Severity:** Medium.

**Description:** `WelcomeLoanLimiter` (Wave 2.8) is a
tested primitive. `ComputeLedger::can_issue_welcome_loan` does NOT
yet consult it — the existing "100 unknown nodes" cap is
effectively still the only gate.

**Why it's open:** integrating requires extending
`can_issue_welcome_loan` with a `bucket: &str` argument, which is
a breaking API change.

**Mitigation:** the existing cap still triggers; a 1 000-node Sybil
swarm would hit it and be rejected regardless.

**Fix tracking:** Phase 17 Wave 2.8-part-2.

### K-006: Fork resync protocol wire messages missing

**Severity:** Low.

**Description:** `ForkDetector` can tell a node it's on a minority
fork, but the full "request the last 1 000 trades from a
majority peer and apply them" protocol needs two new wire
messages (`ResyncRequest`, `ResyncBatch`). They're not implemented.

**Why it's open:** scope-boundedness — Wave 2.5 shipped the
detection + fraud-proof types; the wire protocol is
Wave-2.5-part-2.

**Mitigation:** a minority-forked node currently logs the
divergence; an operator can manually trigger a full snapshot
replay via CLI.

**Fix tracking:** Phase 17 Wave 2.5-part-2.

### K-007: Stake-proven bonus in welcome-loan limiter has no enforcement path

**Severity:** Informational.

**Description:** `WelcomeLoanLimiter::can_issue(bucket,
stake_proven, now_ms)` accepts a `stake_proven: bool` flag but
callers always pass `false` today. Until Wave 2.7's real Base
client lands, there's no verifiable "this peer has staked L2
collateral" signal.

**Why it's open:** depends on chain anchor real-deploy.

**Mitigation:** the cap currently applies uniformly — no peer
unfairly receives the 10× bonus.

**Fix tracking:** Phase 17 Wave 2.8-part-2 after K-003 resolves.

### K-008: Hybrid-chain resync on anchor tx fail retries indefinitely

**Severity:** Low.

**Description:** `Anchorer::run` retries a failed batch submission
on the next tick. There is no backoff; a persistently failing
chain endpoint produces a log every `anchor_interval_secs`.

**Why it's open:** no production consumer has hit this yet (Mock
never fails). Real-chain deployment (post Wave 2.7) will need
exponential backoff.

**Mitigation:** logs are rate-limitable at the operator tracing
level.

**Fix tracking:** Wave 2.7-part-2.

### K-009: `record_audit_failure_slash` double-locks briefly during gossip burst

**Severity:** Low.

**Description:** In a burst of audit responses arriving
concurrently, the handler acquires the ledger lock, then the
staking lock. Lock ordering is consistent (ledger → staking) so
deadlock is impossible, but contention can spike briefly.

**Why it's open:** not yet observed in practice; optimization
candidate if a seed node starts thrashing.

**Fix tracking:** performance pass, post-audit.

## Issues considered and explicitly NOT fixed

### N-001: HMAC integrity key is hard-coded per build

The `HMAC-SHA256` wrapper on the ledger snapshot uses a static key
known at compile time. This is by design: the HMAC is a tamper
detector against naive edits to the JSON file, not a cryptographic
moat. An attacker with write access to the disk can rebuild tirami
with their own key and forge the HMAC. The real integrity comes
from the signed trade records within, not the outer HMAC.

If the auditor recommends upgrading to a user-supplied key, we
can; until then this is noted for transparency.

### N-002: `MockPqVerifier` is trivially forgeable

By design. The name is `Mock*`, the scaffold is behind
`config.pq_signatures = false`, and the doc comment says
"DO NOT USE IN PRODUCTION". Not a real issue.

### N-003: No rate limit on `/v1/tirami/balance`

The endpoint is rate-limited by the general `forge_rate_limiter`
(30 req/sec/token), which is coarse but adequate. Per-endpoint
granularity is Phase-18 work.

## How to update this file

Every security-related code change:
1. Move the corresponding K-### entry to "Resolved" with the wave/PR.
2. If a new issue is discovered during a code change, open a new
   K-### entry before closing the PR.
3. Never delete resolved entries — they're the provenance trail.
