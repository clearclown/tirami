# Phase 21 — Closing the "scaffolded" gaps

> Phase 20 made the AI-agent action economy real. Phase 21 closes the
> remaining items the public README's **Status Honesty** section
> flags as 🟡 *scaffolded but not production-wired*. The biggest one
> is **stake-required mining enforcement**: a function that exists
> but is not yet consulted at the place it matters.

Date: 2026-05-17. Status: Wave 1 in flight.

---

## What's still 🟡 after Phase 20

From `README.md` § Status Honesty:

- 🟡 zkML proof-of-inference (`MockBackend` only) — Phase 22 territory; the real `ezkl` / `risc0` backends are their own multi-week project.
- 🟡 ML-DSA post-quantum hybrid signatures (`pq_signatures = false` by default, blocked on the iroh dependency chain) — out of scope for this Phase.
- 🟡 TEE attestation (`tirami-attestation` scaffold only) — operator-infra problem; not protocol-level.
- 🟡 **Stake-required mining enforcement** — Phase 18.2 designed `can_provide_inference`, but no production call site consults it. **This is what Phase 21 fixes.**
- 🟡 Daemon-mode worker `gossip-recv` loop (issue #88) — separate engineering, tracked elsewhere.

The function `tirami_ledger::ComputeLedger::can_provide_inference` has
existed since Phase 18.2 with the right shape and the right tests,
but **nothing in any HTTP or P2P path actually calls it before
recording a trade**. Until Phase 21, the only place it ran was in
unit tests. That means the protocol *advertised* a Sybil-resistant
property the implementation did not enforce.

## Phase 21 plan

Three waves:

### Wave 1 — Optional enforcement gate (this PR)

- **New `Config.stake_gate_enabled: bool`**, defaulting to `false` for
  backwards-compat. Operators who want to flip the gate on today can.
- **New `ComputeLedger::inference_eligibility`** — same logic as the
  existing `can_provide_inference` bool but returning a structured
  `Result<InferenceEligibility, InferenceIneligible>` so the HTTP
  layer can surface a machine-readable reason in 403 bodies.
- **Wire the gate into `POST /v1/chat/completions`**: when
  `stake_gate_enabled` is `true` AND the local node fails the
  eligibility check, return 403 with a body like:
  ```json
  {
    "error": {
      "type": "stake_gate_denied",
      "code": "stake_required" | "previously_slashed",
      "message": "node consumed stakeless earn cap (contributed 10 ≥ 10 TRM) and current stake 0 < required 100 TRM"
    }
  }
  ```
- **Discovery manifest** now carries a `policy.stake_gate_enabled`
  flag so an agent learns BEFORE its first request whether this node
  enforces stake. The agent can then pick a different provider, post
  stake, or claim a welcome loan rather than discovering the
  requirement via a 403.

This wave is strictly additive. Existing nodes upgrade with zero
behavioural change (gate stays off) until the operator opts in.

### Wave 2 — Welcome-loan counts as effective stake; flip the default ✅ shipped

Until Wave 2, a fresh node with no stake had only a 10 TRM stakeless
window before the gate would refuse to serve. A welcome loan
(1 000 TRM at 0 % for 72 h) was advertised in `lending.rs` constants
but never materialised on the ledger — `can_provide_inference` did
not consult any loan state. Wave 2 closes the gap end-to-end:

- **New `WelcomeLoanGrant` type** + new
  `ComputeLedger.welcome_loans: HashMap<NodeId, WelcomeLoanGrant>`
  field. Tracks granted, expires-at-ms, repaid flag.
- **New `ComputeLedger::grant_welcome_loan(node_id, bucket, now_ms)`**
  performs the eligibility check (sunset epoch, no existing balance),
  records the grant, inserts a zero-balance entry so the single-
  grant-per-node rule fires for any retry, and bumps the Sybil
  rate-limit window for the supplied bucket.
- **`InferenceEligibility` gains a `WelcomeLoan` variant**. The
  verdict order is now: real stake → previously-slashed →
  **welcome loan (unrepaid, unexpired)** → bootstrap window → deny.
- **`POST /v1/tirami/agent/claim-welcome`** (new endpoint). Body
  carries an optional `bucket` for the Sybil window. Returns the
  `WelcomeLoanGrant` (principal, granted_at_ms, expires_at_ms).
  Reachable with a DID-issued bearer token from Phase 20 Wave 5,
  so an autonomous agent can claim without admin scope.
- **`PolicySpec` extended** — manifest now carries
  `welcome_loan_amount_trm`, `welcome_loan_term_hours`, and
  `welcome_loan_available` so an agent knows the bootstrap path
  exists before its first inference call.
- **`Config.stake_gate_enabled` default flipped to `true`.**
  Fresh deploys now enforce stake-required mining out of the box.
  The bootstrap window + welcome-loan auto-claim together cover
  the legitimate-newcomer path; operators with custom flows that
  pre-inflate contributions past the cap without staking can set
  the flag back to `false` explicitly.

Error response shape on duplicate claim:

```json
{
  "error": {
    "type": "welcome_loan_denied",
    "code": "already_has_balance" | "sunset_reached" | "sybil_ceiling",
    "message": "..."
  }
}
```

Mapped to status codes 409 / 410 / 429 respectively.

### Wave 3 — Stake gate on the P2P trade-recording path

The Wave 1 / Wave 2 gate sits on the HTTP serving path. The P2P
`broadcast_trade` / `handle_trade_gossip` path in `pipeline.rs` is
where a gossiped trade from a remote provider lands locally. Wave 3
adds the gate there too:

- When receiving a gossiped signed trade, also verify the provider
  was eligible at the time of trade (per the receiver's view of
  staking + slashing).
- Reject the trade record if not — keeps the local ledger free of
  inflation from Sybil-staked nodes elsewhere on the mesh.

---

## Wave 1 — endpoints + types delivered in this PR

### New: `Config.stake_gate_enabled`

```rust
// crates/tirami-core/src/config.rs
#[serde(default)]
pub stake_gate_enabled: bool,  // default false
```

### New: structured eligibility verdict

```rust
// crates/tirami-ledger/src/ledger.rs
pub enum InferenceEligibility {
    Staked { staked_amount: u64 },
    BootstrapWindow { contributed_so_far: u64, cap_trm: u64 },
}

pub enum InferenceIneligible {
    PreviouslySlashed,
    StakeRequired {
        staked_amount: u64,
        required: u64,
        cumulative_contributed: u64,
        cap_trm: u64,
    },
}

impl ComputeLedger {
    pub fn inference_eligibility(
        &self,
        node_id: &NodeId,
        staking: &StakingPool,
        now_ms: u64,
    ) -> Result<InferenceEligibility, InferenceIneligible> { /* … */ }
}
```

### Wired into `POST /v1/chat/completions`

Gate fires after request validation, before model dispatch. Three
verdicts:

| Verdict | Response |
|---|---|
| `Ok(Staked { … })` | request continues normally |
| `Ok(BootstrapWindow { … })` | request continues normally |
| `Err(InferenceIneligible::StakeRequired { … })` | `403` with `code: "stake_required"` |
| `Err(InferenceIneligible::PreviouslySlashed)` | `403` with `code: "previously_slashed"` |

### Discovery manifest

```json
{
  …
  "policy": {
    "stake_gate_enabled": true
  },
  …
}
```

An autonomous agent reading this manifest before its first request
can pre-emptively post stake (or call `POST /v1/tirami/su/stake`,
or claim a welcome loan when Wave 2 lands) instead of guessing.

---

## What Wave 1 explicitly does NOT do

- Flipping the default to `true`. Today's behaviour is unchanged for
  any existing deploy that didn't set the flag.
- Welcome-loan-as-stake semantics. A fresh node past the 10 TRM
  stakeless cap is currently rejected even if it has an active
  1,000-TRM welcome loan — fixed in Wave 2.
- Stake check on the gossip-receive side. Wave 3.
- Automatic stake purchase. Even with the welcome-loan path in
  Wave 2, the operator must explicitly call the claim endpoint;
  no implicit minting.
