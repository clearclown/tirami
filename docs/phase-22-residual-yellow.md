# Phase 22 — closing the residual 🟡 items

> Phase 21 turned **stake-required mining** from 🟡 *scaffolded* into
> ✅ *enforced* on both the HTTP serving path and the P2P
> gossip-receive path. The README's Status Honesty section still
> carries four 🟡 markers; Phase 22 is the staging ground for
> closing them one at a time.

Date: 2026-05-17.

## Status Honesty items still 🟡 entering Phase 22

1. **NIP-90 publish** — the WebSocket transport (`agora_relay::publish_event`) and the event builder (`Nip90Publisher::build_advertisement_event`) have existed since Phase 9, but the events they produced were **unsigned** — NIP-01 requires a BIP-340 Schnorr signature, and a real Nostr relay drops unsigned events.
2. **zkML real backends** — `tirami-zkml-bench` carries only a `MockBackend`. `ezkl` / `risc0` integration is multi-week scope.
3. **Welcome-loan repayment / settlement** — the Phase 21 Wave 2 grant has an `expires_at_ms` field but no automatic settlement at expiry. Eligibility naturally falls through, but the grant record lingers in memory.
4. **PersonalAgent.wallet ↔ AgentIdentity link** — today the wallet still points at the machine's Ed25519 NodeId. Migrating it to derive from a Phase-20-Wave-4 `AgentIdentity` would let an agent move across machines without losing access to its accumulated TRM in `/v1/tirami/balance`.

Phase 22 picks them off one wave at a time.

---

## Wave 1 — NIP-90 Schnorr signing ✅ shipped

### What changed

- **New workspace dep** `secp256k1 = "0.29"` (matches the version `ldk-node` already pulls in transitively — no second copy of the native libsecp256k1). Features: `rand` for keypair generation, `global-context` so the `Secp256k1` ctx can be shared.
- **New module `tirami_ledger::nostr`**:
  - `NostrIdentity` — secp256k1 keypair scoped to Nostr publishing. **Distinct** from the Ed25519 node identity AND from the Phase-20-Wave-4 `AgentIdentity` DID; a node that wants to advertise on Nostr generates one of these and the rest of the protocol is unaffected.
  - `NostrIdentity::sign_event(event: Value) -> Result<Value, NostrError>` — takes a partially-built NIP-01 event (with `kind`, `created_at`, `tags`, `content` set), computes the canonical event id per NIP-01 (`sha256(json([0, pubkey, created_at, kind, tags, content]))`), signs the id with BIP-340 Schnorr, and returns a complete event ready to ship over `agora_relay::publish_event`.
  - `verify_event(&Value) -> Result<(), NostrError>` — free function that re-derives the id from the event body and verifies the BIP-340 signature. Lets relays / consumers verify without holding a `NostrIdentity` instance.
- **`Nip90Publisher::publish_signed_advertisement`** — new method that wraps `build_advertisement_event` → `NostrIdentity::sign_event` → `agora_relay::publish_event` in one call. The legacy `publish_advertisement` is kept (with a 🟡 caveat in its rustdoc) so existing call sites compile, but new code should prefer the signed variant.

### Verdict matrix

| Operation | Pre-Wave-1 | Post-Wave-1 |
|---|---|---|
| Build event JSON | ✅ | ✅ |
| Send over WebSocket | ✅ | ✅ |
| Pass NIP-01 signature check at the relay | ❌ (silently dropped) | ✅ |

### Tests

9 new tests, all green:
- Unit tests on `NostrIdentity` (8): pubkey-is-32-byte-hex; sign-event-attaches-id-pubkey-sig; sign→verify round-trip; tampered content fails verification; tampered signature fails verification; substituted pubkey fails verification; round-trip from secret bytes preserves pubkey; different identities yield different event ids for the same body.
- Integration test on `Nip90Publisher` (1): a signed advertisement event passes `verify_event` AND its `pubkey` field reflects the signer (not the agent's Ed25519 `node_pubkey_hex` from the advertisement payload).

Workspace: **1,316 passed, 0 failed** (was 1,307 → +9 new).

### What Wave 1 explicitly does NOT do

- **Identity-management surface** for `NostrIdentity`. Wave 1 ships the cryptographic primitive only; persistent keys, export/import, and integration into `AppState` come in Wave 1.5.
- **Operator HTTP endpoint** to trigger a `publish_signed_advertisement`. Library-level call exists; surfacing it via `POST /v1/tirami/agora/publish` is a follow-up.
- **Cross-DID linkage**. The `did:tirami:<hex>` for the agent and the secp256k1 Nostr pubkey are separate identifiers by design — bridging them (e.g. via a self-signed NIP-39 identity proof) is its own scoping decision.

---

## Wave 2 — welcome-loan settlement loop ✅ shipped

The Phase 21 Wave 2 grant carried `granted_at_ms` and
`expires_at_ms` but no automatic settlement at the 72-hour mark.
`InferenceEligibility` correctly stopped honouring expired grants,
but the map kept growing forever and a borrower who claimed
eligibility and produced *zero* contribution during the window left
no audit trail behind. Wave 2 closes both gaps.

### What changed

- **New `WelcomeLoanGrant.defaulted: bool` field** (`#[serde(default)]`
  so existing snapshots stay loadable).
- **New `ComputeLedger::settle_expired_welcome_loans(now_ms)` sweep**
  that iterates every grant where `expires_at_ms <= now_ms && !repaid
  && !defaulted` and:
  - flips `repaid = true` if the borrower has `contributed > 0`
    (productively used the window), OR
  - flips `defaulted = true` AND appends a `SlashEvent { reason =
    "welcome-loan-default", burned_trm = 0, trust_penalty = 0.0 }`
    if `contributed == 0` (Sybil-like signal — claimed eligibility,
    served nothing).
- **`InferenceEligibility` skips defaulted grants** alongside the
  existing `repaid` check. A defaulted borrower's verdict surfaces
  as `PreviouslySlashed` via the slash event, blocking them from the
  stakeless bootstrap path permanently. Real stake is the only
  recovery route (matches the Phase 18.2 constitutional rule).
- **New `WelcomeLoanSettlementReport`** typed return for the sweep
  (`settled_count`, `repaid_count`, `defaulted_count`).
- **New `Config.welcome_loan_settle_interval_secs`** (default `300`,
  clamped to ≥ 60 at spawn time). Plumbed into
  `TiramiNode::spawn_welcome_settle_loop` alongside the existing
  slashing / checkpoint loops.
- **`spawn_welcome_settle_loop`** persists the ledger after any
  non-empty sweep so a restart doesn't lose the audit trail.

### What this means semantically

A welcome loan is **a 72-hour eligibility window**. Wave 2 makes
the window's closure unambiguous and audited:

- "I served some work during the window" → grant is `repaid` and
  the audit record retains it.
- "I claimed the window and did nothing" → grant is `defaulted`,
  a slash event is recorded, and the stakeless bootstrap path is
  closed for this borrower. They can only re-enter the network by
  posting real stake.

### Tests

6 new tests in `tirami-ledger`, all green:
- `phase22_settle_marks_borrower_with_earnings_as_repaid`
- `phase22_settle_marks_zero_contribution_as_defaulted_and_records_slash_event`
- `phase22_settle_skips_already_settled_grants`
- `phase22_settle_does_not_touch_grants_still_within_window`
- `phase22_eligibility_rejects_defaulted_grant`
- `phase22_settle_skips_explicitly_repaid_grants`

Workspace: **1,322 passed, 0 failed** (was 1,316 → +6 new).

### Smoke

`tirami node`'s `spawn_welcome_settle_loop` runs every 300 s by
default; smoke for a 5-min cadence is impractical in CI. The
in-memory state transition is fully covered by the unit tests and
the loop's spawn is verified at boot (no panic / config-default
error).

## Wave 3 — agora Nostr publish HTTP surface ✅ shipped

Wave 1 shipped the cryptographic primitives (`NostrIdentity`,
`sign_event`, `verify_event`, `publish_signed_advertisement` library
call) but no HTTP surface to drive them. Wave 3 closes the loop:
an autonomous agent that has authenticated via Phase 20 Wave 5
(DID-signed bearer token) can now bootstrap a Nostr identity, sign
events, and publish to a real Nostr relay — all without
human-shared secrets.

### Endpoints (all auth-required)

| Method | Path | Purpose |
|---|---|---|
| POST | `/v1/tirami/agora/nostr/init` | Idempotent bootstrap of a per-node `NostrIdentity`. Returns the x-only pubkey. |
| GET | `/v1/tirami/agora/nostr` | Current pubkey + bootstrap state. |
| POST | `/v1/tirami/agora/nostr/sign-event` | Sign an arbitrary partially-built NIP-01 event. |
| POST | `/v1/tirami/agora/publish` | Build a kind-31990 ad from a `ProviderAdvertisement`, sign it, optionally ship to a relay. `dry_run: true` skips the relay round-trip and returns the signed event for inspection. |

### AppState + manifest

- New slot `AppState.nostr_identity: Arc<Mutex<Option<NostrIdentity>>>`. In-memory only.
- `PolicySpec.nostr_pubkey: Option<String>` — manifest now surfaces the x-only pubkey when bootstrapped, `None` otherwise. Non-blocking read so a busy node returns `None` rather than blocking the manifest GET.
- Four new `ActionDescriptor` entries advertised unconditionally so a discovering agent sees the publishing surface even before it bootstraps.

### Test coverage uplift

19 new tests in `tirami-node`, all green:

- 4 unit tests on the handler types (default_timeout / view serialises / dry_run default / dry_run can be set).
- 13 integration tests:
  - `nostr_init_then_status_round_trip`
  - `nostr_init_is_idempotent`
  - `nostr_sign_without_init_returns_412`
  - `nostr_sign_event_returns_verifiable_signature` (end-to-end through `tirami_ledger::verify_nostr_event`)
  - `nostr_sign_event_rejects_malformed_event`
  - `agora_publish_dry_run_returns_signed_event_without_relay`
  - `agora_publish_without_init_returns_412`
  - `agora_publish_rejects_bad_pubkey_length`
  - `agora_publish_with_dead_relay_returns_502`
  - `agora_publish_default_relay_url_path_resolves` (network-tolerant assertion)
  - `manifest_exposes_nostr_pubkey_and_wave_3_actions`
  - `two_nodes_produce_independent_signatures`
  - `agora_publish_event_has_kind_31990_and_model_tags`
- 2 backfill integration tests covering thin Wave 1 / Wave 2 areas:
  - `library_signed_advertisement_round_trips_through_library_verify`
  - `welcome_loan_default_appears_in_slash_events_endpoint`

Workspace: **1,341 passed, 0 failed** (was 1,322 → +19 new).

### Smoke

End-to-end against `tirami node --api-token t`:

```
[1] GET  /v1/tirami/agora/nostr                          → {initialized:false, pubkey_hex:null}
[2] POST /v1/tirami/agora/nostr/init                     → {initialized:true,  pubkey_hex:"072e6aeb..."}
[3] GET  /.well-known/tirami-agent.json                  → policy.nostr_pubkey = "072e6aeb..."
[4] POST /v1/tirami/agora/publish (dry_run=true)         → {
                                                            event.kind:        31990,
                                                            event.pubkey:      "072e6aeb...",
                                                            event.id len:      64,
                                                            event.sig len:     128,
                                                            relay_accepted:    false (suppressed by dry_run),
                                                            relay_url:         null
                                                          }
```

### What this PR does NOT do

- **On-disk persistence of the Nostr keypair.** A restart drops it; the next `init` call gets a new keypair. Phase 23 territory.
- **Subscribing to incoming events.** Only publish in this surface; consumer flows are out of scope.
- **NIP-39 identity binding** between the secp256k1 Nostr pubkey and a `did:tirami:` identity. Wave 4+.

## Phase 22 residual 🟡 → 🟢 status after Wave 3

| Item | Status | Notes |
|---|---|---|
| NIP-90 publish | 🟢 | Library (Wave 1) + HTTP surface (Wave 3) shipped. Live relay smoke deferred. |
| Welcome-loan settlement | 🟢 | Wave 2: periodic loop + default tracking. |
| zkML real backends | 🟡 | Phase 23. Significant scope (ezkl / risc0 dep chain). |
| PersonalAgent.wallet ↔ AgentIdentity link | 🟡 | Phase 23 candidate. Refactor with snapshot migration. |

### Out of Phase 22 scope (Phase 23 / 24 territory)

- **zkML real backends** (`ezkl` / `risc0`). Real proof-of-inference is the main remaining 🟡 → ✅ jump and warrants its own Phase. Likely a multi-PR effort spanning crate integration, proof-vs-trade plumbing, governance ratchet wiring, and per-model benchmark calibration.
