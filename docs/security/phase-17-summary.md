# Phase 17 Security Hardening — Condensed Summary

> Written for an external auditor reading this before engagement.
> For the full per-wave detail see `../../CHANGELOG.md`.

**Date:** 2026-04-18 (close-out).
**Scope:** Make Tirami safe to release on a public adversarial
network at 100-1 000 nodes of scale.
**Status:** Code-complete on every buildable defense. External
gates (audit, live deploy, 30-day observation, multi-sig, bounty
launch, upstream dep reconciliation) remain.

---

## What Phase 17 delivered

24 primitives across 4 waves, 180 new unit tests, CI workflows for
both Rust and Solidity sides, full operator documentation.

### Wave 1 — P0 Critical Integrity (all in production paths)

| # | Title | Effect |
|---|-------|--------|
| 1.1 | TradeRecord v2 with 128-bit nonce | Replay of a signed trade produces a distinct record, not a duplicate |
| 1.2 | `execute_signed_trade` + nonce dedup | Gossip flow is replay-safe; nonce cache rebuilt from trade_log on restart |
| 1.3 | Slashing wired into daemon loop | `apply_slash` was dead code pre-Phase-17; now fires every 5 min |
| 1.4 | AuditVerdict → slashing bridge | Failed audit burns 30% of stake + persisted event |
| 1.5 | Per-node scoped API tokens | Leaked ReadOnly token no longer compromises Admin operations |
| 1.6 | Ed25519 + ML-DSA hybrid signature scaffold | Post-quantum preparation (mock verifier until ml-dsa stabilizes) |

### Wave 2 — P1 Scale Hardening (primitives + wire-up)

| # | Title | Effect |
|---|-------|--------|
| 2.1 | SPoRA random-layer audit | Truncated-model attack forces full-weight loading |
| 2.2 | Probabilistic heavy-audit scaffold | 1% sampling + 2/3 validator quorum for deep checks |
| 2.3 | Per-ASN rate limiter (+ wire-up 4.4) | Cloud Sybil collapses from N× to 1× per ASN |
| 2.4 | Trade-log snapshotting (+ daemon loop 4.3) | Memory bounded by retain window × trade rate |
| 2.5 | Fork detection + nonce-fraud proofs | Double-signed nonce collision is broadcast-provable |
| 2.6 | PeerRegistry LRU | Unbounded memory creep closed at default 10k cap |
| 2.7 | Base Sepolia scaffold + runbook | Deploy is a single `forge script` away, gated on audit |
| 2.8 | Welcome-loan per-bucket Sybil (+ 4.1 wire-up) | ASN-level onboarding cap with stake-proven 10× bonus |

### Wave 3 — P2 Hostile-Environment Readiness

| # | Title | Effect |
|---|-------|--------|
| 3.1 | TEE attestation scaffold | Optional premium tier (Apple SE / NVIDIA H100 / Intel SGX) |
| 3.2 | Kani formal verification | 10 invariants over replay, canonical bytes, slash monotonicity |
| 3.3 | External audit preparation | Scope doc + candidate shortlist + mainnet gate checklist |
| 3.4 | DDoS mitigation (+ 4.2 wire-up) | `max_concurrent_connections` enforced + 7-section ops guide |
| 3.5 | Key rotation scaffold | `NodeIdentity` with epochs; historical verifier accepts pre-rotation sigs |
| 3.6 | Bug bounty framework | `SECURITY.md` scale + rules + PGP setup guide |

### Wave 4 — Production Wire-up

Wave 4 closes every "primitive shipped but not wired" gap from
Waves 2-3, plus one contracts fix found during pre-deploy rehearsal.

| # | Title | Effect |
|---|-------|--------|
| 4.1 | WelcomeLoanLimiter wired into ComputeLedger | Sybil cap actually gates grants (not just sits in a module) |
| 4.2 | `max_concurrent_connections` enforced in transport | fd exhaustion defense is live |
| 4.3 | `spawn_checkpoint_loop` in daemon | Trade-log auto-seals every hour |
| 4.4 | AsnRateLimiter in transport (initially thought blocked) | Cloud-Sybil defense is live once operator installs a resolver |
| 4.5 | API self-sign path — architectural clarification | No behavior change; doc comments explain why these stay unsigned |
| 4.6 | CI + contracts polish | Kani + Foundry workflows; TiramiBridge first-mint deadlock fixed |

---

## Threat coverage changes (see `threat-model-v2.md` for full detail)

- **T18 (dead slashing code)** — Closed by Wave 1.3 + 1.4.
- **T19 (single-secret bearer token)** — Closed by Wave 1.5.
- **T20 (CRQC horizon)** — Scaffolded by Wave 1.6; residual Medium
  until the real ML-DSA backend ships.
- **T21 (PeerRegistry memory)** — Closed by Wave 2.6.
- **T22 (trade-log memory)** — Closed by Wave 2.4 + Wave 4.3.
- **T23 (node key compromise without rotation)** — Scaffolded by
  Wave 3.5; residual Medium until `ForgeTransport` is migrated
  to consume `NodeIdentity`.
- **T24 (DDoS)** — Closed by Wave 3.4 + Wave 4.2 + Wave 4.4.
- **T25 (TEE-less peer claims hardware trust)** — Scaffolded by
  Wave 3.1.
- **T26 (no formal verification)** — 10/30 invariants landed in
  Wave 3.2; target is 30 before external audit.
- **T27 (no responsible disclosure)** — Closed by Wave 3.6.

T10 (replay), T11 (free-tier abuse), T14 (byzantine inference)
all have expanded defenses — see the individual waves.

---

## What remains external (auditor should not expect these to be
code-closable from Tirami's side)

1. **External professional audit** — this document is part of
   the engagement prep. When you're reading this, that's you.
2. **Base Sepolia live deploy** — `forge test` 15/15 locally,
   runbook is complete, needs a funded wallet + RPC URL from
   the operator. Mainnet is blocked behind this audit plus
   30-day Sepolia stability + multi-sig custody + ≥ 30-day
   bounty program.
3. **ML-DSA real backend** — pending reconciliation between
   `ml-dsa 0.1.0-rc.x` (wants `digest 0.11.0`) and iroh 0.97
   (pins `digest 0.11.0-rc.10`). Waiting on either upstream.
4. **TEE hardware bindings** — `security-framework` (Apple SE)
   and `nvml-wrapper` (NVIDIA H100 CC) integrations land when
   operator testbeds on that hardware are available.
5. **30-day Sepolia stability** — can't compress time.
6. **Multi-sig custody** — operator configures pre-deploy.
7. **Bug bounty program launch** — framework ready, PGP key
   generation guide at `pgp-key-setup.md`, needs operator to
   publish the key + open the program.

---

## Signals of readiness

- `cargo test --workspace`: 1 071 / 0 failures.
- `cargo build --workspace`: clean.
- `scripts/verify-impl.sh`: 123 / 123 GREEN.
- `forge test` (in `repos/tirami-contracts/`): 15 / 15 passing.
- `cargo kani --package tirami-ledger`: 10 invariants pass (not yet
  integrated into default CI; see `.github/workflows/kani.yml`).

Every primitive has its own test module with meaningful coverage;
the test count is not padding.

---

## How this doc is meant to be used

- The auditor reads this first for the 10-minute orientation.
- Then reads `audit-scope.md` for rules of engagement.
- Then `threat-model-v2.md` for threat-to-code mapping.
- Then `known-issues.md` to avoid wasting time on disclosed issues.
- Then dives into code, starting from the crates listed in
  `audit-scope.md` §2.1 in priority order.

If you'd like a walking tour of a specific wave, every commit in
the `phase-17/wave-*` branches has a detailed message explaining
the choices made.
