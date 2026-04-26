# Tirami Documentation Index

> Entry point for every doc under `docs/`. Updated 2026-04-27
> after the two-node Tailscale agent-dispatch experiment.

New reader? Start with **concept** → **architecture** → **roadmap**.
Operator? Start with **operator-guide** → **public-testnet-launch** → **security**.
Security researcher? Start with **security/audit-scope** →
**security/threat-model-v2** → **security/known-issues**.

---

## Quick reference

| Role | Start here |
|------|------------|
| First-time reader | [`concept.md`](concept.md) |
| Protocol developer | [`architecture.md`](architecture.md) → [`protocol-spec.md`](protocol-spec.md) |
| Node operator | [`operator-guide.md`](operator-guide.md) → [`security/`](security/) |
| Public testnet operator | [`public-testnet-launch.md`](public-testnet-launch.md) → [`operator-guide.md`](operator-guide.md) |
| AI agent integrator | [`agent-integration.md`](agent-integration.md) |
| Economic reasoner | [`economy.md`](economy.md) → [`monetary-theory.md`](monetary-theory.md) → [`theory-audit-2026-04.md`](theory-audit-2026-04.md) |
| Security researcher / auditor | [`security/audit-scope.md`](security/audit-scope.md) |
| Bug-bounty hunter | [`../SECURITY.md`](../SECURITY.md) |

---

## Concept & theory

- [`concept.md`](concept.md) — why compute is money, the post-marketing economy
- [`monetary-theory.md`](monetary-theory.md) — why TRM works: Soddy + Bitcoin + PoUW synthesis
- [`economy.md`](economy.md) — CU-native economy, Proof of Useful Work, lending
- [`strategy.md`](strategy.md) — competitive positioning, 5-layer architecture
- [`theory-audit-2026-04.md`](theory-audit-2026-04.md) —
  objective historical/economic review after the first private-lab
  remote-agent spend/earn test

## Architecture

- [`architecture.md`](architecture.md) — two-layer design (economic + inference)
- [`protocol-spec.md`](protocol-spec.md) — wire protocol
- [`hybrid-chain-design.md`](hybrid-chain-design.md) — off-chain/on-chain hybrid
- [`bitvm-design.md`](bitvm-design.md) — BitVM fraud-proof path (long-term)

## Phase-specific design

- [`phase-14-design.md`](phase-14-design.md) — Ledger-as-Brain unified scheduler
- [`phase-17-wave-2.7-base-deployment.md`](phase-17-wave-2.7-base-deployment.md) — Base Sepolia deploy runbook

## Operation

- [`operator-guide.md`](operator-guide.md) — install, configure, monitor, back up, DDoS mitigation
- [`public-testnet-launch.md`](public-testnet-launch.md) — Ring 0/Ring 1/Ring 2 launch runbook for worldwide node joins
- [`release-readiness.md`](release-readiness.md) — status honesty,
  scale-tier verdicts, and the 2026-04-26 two-node E2E result
- [`bootstrap.md`](bootstrap.md) — startup, degradation, recovery
- [`faq.md`](faq.md) — frequent questions
- [`compatibility.md`](compatibility.md) — GGUF/model compatibility
- [`migration-guide.md`](migration-guide.md) — upgrade paths between versions

## Integration

- [`agent-integration.md`](agent-integration.md) — SDK, MCP, borrowing workflow, credit building
- [`a2a-payment.md`](a2a-payment.md) — TRM payment extension for A2A / MCP
- [`developer-guide.md`](developer-guide.md) — contributing to the codebase

## Security

All security documentation lives under [`security/`](security/):

- [`security/audit-scope.md`](security/audit-scope.md) — external audit
  scope, candidate auditor shortlist, deliverables, feature freeze
  rules, mainnet gate checklist
- [`security/threat-model-v2.md`](security/threat-model-v2.md) —
  27 threats re-scored with residual risk + Phase-17 mitigations
- [`security/known-issues.md`](security/known-issues.md) — open
  K-### issues, resolved issues, explicitly-accepted trade-offs
- [`security/kani-proofs.md`](security/kani-proofs.md) — formal
  verification proofs + how to run them
- [`security/pgp-key-setup.md`](security/pgp-key-setup.md) —
  operator-owned PGP key generation procedure
- [`threat-model.md`](threat-model.md) — legacy v1 threat model
  (superseded by `security/threat-model-v2.md` but preserved for
  historical context)

## Roadmap & history

- [`roadmap.md`](roadmap.md) — current phase status, upcoming waves
- [`../CHANGELOG.md`](../CHANGELOG.md) — per-phase release notes
- [`THEORY-AUDIT.md`](THEORY-AUDIT.md) — 1:1 mapping between
  `forge-economics` theory and Rust implementation

## Demos & artifacts

- [`e2e-demo-phase-15.md`](e2e-demo-phase-15.md) — end-to-end demo
- [`hn-teaser-draft.md`](hn-teaser-draft.md) — public announcement draft

## Translations

- [`translations/`](translations/) — non-English versions of key docs

---

## Related repositories

- `repos/tirami-contracts/` — TRM ERC-20 + TiramiBridge (Foundry)
- `repos/tirami-economics/` — economic theory, design rationale
  (single source of truth for all economic parameters)
- `repos/tirami-v2/` — v2 reference implementation (scaffold)

## How to navigate

- Every doc starts with a "Status" or "Scope" line so you can
  tell whether it's current, historical, or scaffold-only.
- Cross-references use relative paths so the docs are readable
  from GitHub's web UI without breaking.
- Phase-tagged docs (e.g. `phase-14-design.md`,
  `phase-17-wave-2.7-*.md`) are frozen snapshots of that phase's
  decisions; they are NOT updated when the code evolves. The
  evolving state lives in `architecture.md` and `roadmap.md`.
