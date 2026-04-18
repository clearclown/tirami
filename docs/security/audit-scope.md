# Tirami — External Security Audit Scope

**Version:** Phase 17 Wave 3.3 · 2026-04-18
**Status:** Draft for auditor review. All Phase 17 Waves 1+2 hardening
is in tree; Wave 3 hardening is in progress on branch
`phase-17/wave-3-hostile`.

This document defines the scope, artifacts, and rules of engagement
for Tirami's first external security audit. Feature freeze begins on
the day an auditor is selected; code changes during the audit window
are restricted to fixes for findings from the auditor.

## 1. Why this audit exists

Tirami aspires to be a public, adversarial-network deployable
distributed compute marketplace where compute itself is the currency.
The Phase 17 internal hardening closed every P0/P1 finding from our
own review (see `docs/threat-model.md` + the Phase 17 Delta section);
this audit independently verifies that, catches anything we missed,
and is a pre-condition for any mainnet deployment.

## 2. In-scope artifacts

### 2.1 Rust workspace

Primary focus. All crates in this workspace are in scope:

| Crate | Role | Lines (approx) |
|------|------|----------------|
| `tirami-ledger` | Core economic engine — TRM trades, staking, slashing, checkpointing, fork detection | ~6 000 |
| `tirami-core` | Shared types, config, crypto scaffold | ~1 500 |
| `tirami-net` | iroh QUIC transport, gossip, per-ASN rate limiter | ~2 500 |
| `tirami-node` | HTTP API (`/v1/chat/completions`, `/v1/tirami/*`), scoped API tokens | ~4 000 |
| `tirami-anchor` | Chain-anchor client, Base Sepolia scaffold | ~800 |
| `tirami-proto` | Wire protocol (bincode, 14+ message types) | ~800 |
| `tirami-infer` | Inference engine trait (llama.cpp backend) | ~1 300 |
| `tirami-bank` | L2 financial strategies, futures, portfolio | ~1 800 |
| `tirami-mind` | L3 self-improvement harness | ~1 500 |
| `tirami-agora` | L4 marketplace, reputation, NIP-90 | ~1 200 |

Auditor attention should prioritize in roughly this order:

1. **`tirami-ledger`** — all economic invariants land here
2. **`tirami-node/src/api.rs`** — the attack surface (HTTP)
3. **`tirami-net/src/gossip.rs`** — peer-to-peer replay/dedup
4. **`tirami-proto`** — wire-format validation
5. **`tirami-anchor`** — chain-facing serialization
6. **`tirami-core/src/crypto.rs`** — HybridSignature scaffold

### 2.2 Solidity contracts

Located at `repos/tirami-contracts/`:

- `src/TRM.sol` — ERC-20 with 21 B cap, bridge role, non-mintable
  except by bridge.
- `src/TiramiBridge.sol` — BatchSubmitted event, bridge custody.

Foundry test suite at `test/` (15 tests at the time of writing). The
contracts have NOT been deployed to Base Sepolia yet — that's a
post-audit operator action per
`docs/phase-17-wave-2.7-base-deployment.md`.

### 2.3 Cryptographic primitives

- Ed25519 trade / loan signing via `ed25519-dalek` 2.x (verify_strict
  mode, no malleability).
- HMAC-SHA256 ledger integrity wrapper for snapshot-on-disk.
- SHA-256 Merkle root over trade `canonical_bytes()`.
- HybridSignature scaffold (Ed25519 + `Option<ML-DSA>`), real ML-DSA
  backend deferred to post-audit pending dep-pin resolution.
- Nonce replay cache: bounded per-provider FIFO (10 000 cap), rebuilt
  from `trade_log` on restart.

## 3. Out of scope

- `repos/tirami-v2/` — reference v2 tree used for design checks; NOT
  production code.
- `sdk/python/` — legacy Python SDK, slated for deprecation.
- `crates/tirami-cli` — command-line UX only; no economic
  invariants.
- Existing auditor-completed dependencies: iroh, ed25519-dalek,
  rustcrypto. Findings on these should be reported to upstream.
- Infrastructure / DevOps audit (CI, release signing, dependency
  supply chain). A separate pass.

## 4. Threat model this audit targets

Copy of `docs/threat-model.md` with the Phase 17 Delta appendix
applied. Key threats we want independent validation on:

| ID | Threat | Current posture |
|----|--------|-----------------|
| T1 | Malicious seed node | Ed25519 dual-sig + gossip verification |
| T3 | Sybil attack | Welcome-loan per-ASN limiter + stake-proof bonus |
| T10 | TRM forgery via replay | Nonce replay cache + fraud-proof detection |
| T14 | Inference quality attack | Audit challenge (+ SPoRA layer scope) + slashing |
| T18 | Dead slashing code (pre-audit internal) | Live via `update_trust_penalties` loop |
| T19 | Single-secret bearer token | Scoped API tokens, per-endpoint ACL |
| T20 | CRQC (post-quantum) | HybridSignature scaffold, opt-in via config |

## 5. Out-of-band material

Auditor gets access to:

- `.claude/plans/humble-cooking-thimble.md` — the full Phase 17 plan.
- `docs/threat-model.md` (v2 lives at `docs/security/threat-model-v2.md`).
- `docs/security/known-issues.md` — issues we've identified but
  haven't yet fixed. Transparency to avoid duplicate effort.
- Snapshot of the repo at the agreed commit hash (tag:
  `phase-17/audit-2026-04-xx`).

## 6. Candidate auditors

Selection criteria: Rust + Solidity + cryptography primitives
experience, prior work in distributed economic protocols.
Shortlist:

- **Trail of Bits** — strong Rust + crypto track record; has audited
  Filecoin, Zcash, MobileCoin.
- **OpenZeppelin** — Solidity dominant; has done Rust (Starknet
  cairo-vm etc).
- **Zellic** — Rust + Move experience; Substrate, Aptos.
- **Least Authority** — long cryptography track record; audited
  zcashd, MLS.
- **Runtime Verification** — formal-methods oriented; would pair well
  with Wave 3.2 Kani invariants.

## 7. Ground rules during audit

- **Feature freeze**: no non-fix commits to `main` from the start
  day until the final report is delivered. New work lands on
  `phase-18/*` branches and is not merged until post-audit.
- **Daily syncs**: async written updates; sync call weekly.
- **Finding handling**: High/Critical findings block any public
  deployment; fixes go through `security/` branches with private
  PRs, disclosed after patch is public.
- **Budget transparency**: total audit cost + scope agreed up front;
  extensions require explicit sign-off.

## 8. Deliverables we expect from the auditor

1. Written report with:
   - Executive summary
   - Findings table (severity, title, location, impact, recommendation)
   - Per-finding detail: reproduction, root cause, proposed fix
   - Overall risk assessment
2. Presentation / walkthrough call.
3. Retest of fixes (one round included in base fee).

## 9. Post-audit gates

Before Tirami ships to Base mainnet:

- [ ] All `Critical` and `High` findings resolved and auditor
      has re-verified.
- [ ] `Medium` findings either resolved or documented as accepted
      risk in `docs/security/known-issues.md`.
- [ ] 30-day Sepolia stability (see the Wave 2.7 runbook).
- [ ] Multi-sig custody configured for the deployer wallet.
- [ ] Bug bounty program live for ≥ 30 days with no unresolved
      critical findings.

## 10. Status tracker

| Step | Status |
|------|--------|
| Scope drafted | ✅ 2026-04-18 |
| Auditor shortlisted | ⬜ pending |
| Auditor selected | ⬜ pending |
| Feature freeze starts | ⬜ pending |
| Draft report | ⬜ pending |
| Fixes complete | ⬜ pending |
| Final report | ⬜ pending |
| Sepolia 30-day stability | ⬜ pending |
| Mainnet go / no-go | ⬜ pending |
