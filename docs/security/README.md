# Tirami Security Documentation

> Index for `docs/security/`. Updated 2026-04-18 (post-Phase-17).

Every file here answers a specific question. Pick the one that
matches your role.

---

## Role-based entry points

### 🔒 External security auditor (pre-engagement)

1. [`audit-scope.md`](audit-scope.md) — what's in/out of scope,
   candidate auditor shortlist, deliverables, feature freeze
   rules, mainnet gate checklist.
2. [`threat-model-v2.md`](threat-model-v2.md) — the 27 threats
   this codebase defends against, with current residual risk.
3. [`known-issues.md`](known-issues.md) — issues we've already
   found so you don't waste budget re-finding them. Read this
   before engagement starts.
4. [`phase-17-summary.md`](phase-17-summary.md) — condensed view
   of what Phase 17 delivered and what remains external-gated.

### 🎯 Bug-bounty hunter / security researcher

1. [`../../SECURITY.md`](../../SECURITY.md) — reporting channel,
   SLA, bounty scale, rules of engagement.
2. [`threat-model-v2.md`](threat-model-v2.md) — the threats
   we already know about (T1-T27 + R-001 to R-003 residuals).
3. [`known-issues.md`](known-issues.md) — what's already
   disclosed; findings in these areas won't pay a bounty unless
   they exceed the documented scope.

### 🛠️ Node operator

1. [`../operator-guide.md`](../operator-guide.md) — install,
   configure, monitor, back up. Includes the DDoS mitigation
   runbook (Wave 3.4).
2. [`pgp-key-setup.md`](pgp-key-setup.md) — generating the PGP
   keypair that backs the `SECURITY.md` contact. Operator-owned.
3. [`../phase-17-wave-2.7-base-deployment.md`](../phase-17-wave-2.7-base-deployment.md)
   — Base Sepolia deploy runbook. Mainnet is gated.

### 📐 Implementation / code reviewer

1. [`kani-proofs.md`](kani-proofs.md) — formal-verification
   invariants and how to run them (`cargo kani`).
2. [`threat-model-v2.md`](threat-model-v2.md) — threat-to-code
   mapping. Each threat cites the wave that mitigated it.

---

## File index

| File | Audience | What it answers |
|------|----------|-----------------|
| [`audit-scope.md`](audit-scope.md) | Auditor | "What am I auditing? What isn't in scope?" |
| [`threat-model-v2.md`](threat-model-v2.md) | All | "What threats does this defend against?" |
| [`known-issues.md`](known-issues.md) | All | "What's already known and unfixed?" |
| [`phase-17-summary.md`](phase-17-summary.md) | Auditor | "What changed in Phase 17?" |
| [`kani-proofs.md`](kani-proofs.md) | Reviewer | "How do I run the formal proofs?" |
| [`pgp-key-setup.md`](pgp-key-setup.md) | Operator | "How do I generate the security contact key?" |

## Legacy

[`../threat-model.md`](../threat-model.md) is the Phase-16 threat
model. It is superseded by [`threat-model-v2.md`](threat-model-v2.md)
but preserved for historical context. If the two conflict, v2
wins.

## Updating this documentation

- Every K-### in `known-issues.md` must cite a tracking issue
  or a resolution wave.
- When a K-### resolves, move it to the "Resolved" table with the
  wave that fixed it — never delete the entry; the provenance
  trail is more valuable than a short file.
- Severity judgments are ours; auditors are welcome to disagree
  and we'll update the doc accordingly.
- `audit-scope.md`'s feature-freeze section takes effect the
  day an auditor is selected, not the day this doc is committed.
