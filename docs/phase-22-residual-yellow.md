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

## Wave 2 — TBD

Candidates in priority order:

- **Welcome-loan settlement loop** (mechanical, bounded scope). Periodic task scans `welcome_loans` for expired grants and marks them as such; bonus: garbage-collects the map and records a small reputation event for any grant that produced zero serving activity during its 72-hour window.
- **PersonalAgent wallet → AgentIdentity link**. Refactor `PersonalAgent.wallet: NodeId` so it can be derived from a loaded `AgentIdentity`. Backwards-compat important; needs careful migration of existing snapshots.
- **`POST /v1/tirami/agora/publish`** — surfacing the Wave-1 primitive via HTTP so an autonomous agent can announce itself on a public Nostr relay. Adds an `AppState.nostr_identity: Arc<Mutex<Option<NostrIdentity>>>` slot.

### Out of Phase 22 scope (Phase 23 / 24 territory)

- **zkML real backends** (`ezkl` / `risc0`). Real proof-of-inference is the main remaining 🟡 → ✅ jump and warrants its own Phase. Likely a multi-PR effort spanning crate integration, proof-vs-trade plumbing, governance ratchet wiring, and per-model benchmark calibration.
