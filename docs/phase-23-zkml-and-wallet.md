# Phase 23 — closing the last residual 🟡 items

> Phase 22 left two 🟡 markers in the Status Honesty section:
> 1. **zkML real backends** (`ezkl` / `risc0`) — multi-week scope.
> 2. **PersonalAgent.wallet ↔ AgentIdentity link** — bounded refactor.
>
> Phase 23 starts with the bounded item (Wave 1) and leaves zkML for a
> later wave or a dedicated Phase 24.

## Wave 1 — PersonalAgent.wallet ↔ AgentIdentity link ✅ shipped

### Problem

Until Wave 1, `PersonalAgent.wallet: NodeId` always held the host
machine's Ed25519 public key (the `~/.tirami/node.key`-derived
`NodeId`). The Phase-20-Wave-4 `AgentIdentity` (with its portable
`did:tirami:<hex>`) lived alongside it but had no effect on trade
attribution. An agent that exported its identity and imported it on a
different host kept its DID but **earned and spent as the new host's
machine identity** — the portability was decorative.

### What changed

- **New enum `tirami_mind::WalletSource`** with two variants:
  - `MachineNode` (pre-Wave-1 default; wallet derives from the
    machine key).
  - `AgentIdentity` (post-Wave-1 when an `AgentIdentity` is loaded;
    wallet derives from the DID pubkey).
- **New field `PersonalAgent.wallet_source: WalletSource`**, with
  `#[serde(default = "default_wallet_source")]` so pre-Wave-1
  snapshots stay loadable and default to `MachineNode`.
- **New method `PersonalAgent::rebind_wallet_from_agent_identity`**
  (idempotent on same-pubkey-and-source; tally-resetting on
  pubkey change because "today's earnings" belonged to a different
  actor before the rebind).
- **Rebind hook in the `agent_identity` HTTP handler**:
  `POST /v1/tirami/agent/identity/init` and
  `POST /v1/tirami/agent/identity/import` both propagate the
  new pubkey into `PersonalAgent.wallet` via a private helper. A
  node without a configured PersonalAgent is a clean no-op.
- **`/v1/tirami/agent/status` response gained `wallet_source`** —
  clients can tell at a glance whether trades attribute to the
  machine (`"machine_node"`) or the portable DID
  (`"agent_identity"`). Enum is serialised in snake_case so JSON
  consumers don't have to think about it.

### What this PR does NOT do (Wave 2+ deferrals)

- **P2P trade signing**. Outbound P2P trades are still signed with
  the **machine** SigningKey, not the AgentIdentity SigningKey.
  That's a deeper refactor (the signing key has to follow the
  identity, which means rebinding the identity must also rebind
  the signer in `tirami-net`). Wave 1 only moves the **wallet
  attribution** to the DID — that's what shows up on
  `/v1/tirami/balance` and the unsigned self-bookkeeping trades.
  The full crypto-identity unification is Wave 2 work.
- **Migration of historical trades** — pre-rebind trades stay
  attributed to the machine NodeId on the ledger. That's an audit
  trail of who-was-who-when, intentionally preserved.

### Verdict matrix

| Operation | Pre-Wave-1 | Post-Wave-1 |
|---|---|---|
| `POST /agent/identity/init` returns DID | ✅ | ✅ |
| `/.well-known/tirami-agent.json` shows `agent_did` | ✅ | ✅ |
| `PersonalAgent.wallet == AgentIdentity pubkey` after init | ❌ (still machine key) | ✅ |
| `/v1/tirami/agent/status` carries `wallet_source` | ❌ | ✅ |
| Wallet portability across nodes via export/import | partial (DID portable, wallet not) | ✅ |
| P2P trade *signing* migrates to AgentIdentity key | ❌ | ❌ (Wave 2) |

### Tests

8 new tests, all green:
- 2 unit tests on `PersonalAgent::rebind_wallet_from_agent_identity`
  (idempotent same-pk-and-source no-op; different-pk resets tallies).
- 6 integration tests on the HTTP surface (pre-init reports
  `machine_node`; init rebinds; idempotent init preserves the
  rebound wallet; cross-node import on a fresh node rebinds;
  init without a PersonalAgent is a safe no-op; status response
  uses snake_case for the enum).

Workspace: **1,349 passed, 0 failed** (was 1,341 → +8 new).

### Smoke

Against a release-mode `tirami start --port 3017`:

| Step | Observation |
|---|---|
| 1. Pre-init `GET /v1/tirami/agent/status` | `wallet_source: machine_node`, wallet `30156cf7…` (machine NodeId) |
| 2. `POST /v1/tirami/agent/identity/init` | Returns `did:tirami:9c4110ad…` |
| 3. Post-init `GET /v1/tirami/agent/status` | `wallet_source: agent_identity`, wallet `9c4110ad…` — matches DID pubkey byte-for-byte |

## Wave 2 — P2P trade signer follows AgentIdentity ✅ shipped

Wave 1 moved wallet **attribution** to the DID but left the
**signing key** on the host machine. A SignedTradeRecord that
appeared to come from a DID was actually signed by the host's
Ed25519 key — verifiable to anyone who knew the machine pubkey,
not to anyone who only knew the DID. Wave 2 fixes the signer.

### What changed

- **New `TiramiNode.agent_identity: Arc<Mutex<Option<AgentIdentity>>>`
  field.** Defaults to `None`. The HTTP `agent/identity/init` and
  `…/import` handlers populate it via the same Arc (the
  Wave-2.5 plumbing that shares this slot between AppState and
  TiramiNode is the only remaining structural cleanup).
- **New `pipeline::resolve_outbound_trade_signing` helper.** Pure
  function that takes the canonical bytes, the machine NodeId,
  a machine-signing closure, and an optional `&AgentIdentity`,
  and returns `(provider_node_id, signature_bytes)`:
  - `Some(agent)` → provider = agent pubkey, sig = agent.sign(canonical)
  - `None`        → provider = machine_node_id, sig = machine_sign(canonical)
- **`PipelineCoordinator::run_seed` gained an `agent_identity` Arc
  parameter.** Per-request the recv loop snapshots the current
  AgentIdentity (cloned by value) into the inference-handling
  spawn so the trade signer follows the *current* identity, not
  whichever one happened to exist at boot.
- **`handle_inference` resolves the effective provider id BEFORE
  serialising `canonical_bytes`** because `provider` is part of the
  canonical pre-image. Without this ordering the signature wouldn't
  verify.

### Properties guaranteed

| Property | Pre-Wave-2 | Post-Wave-2 |
|---|---|---|
| `SignedTradeRecord.provider == agent_pubkey` after init | ❌ | ✅ |
| `signed.verify()` confirms the signature comes from the agent | ❌ | ✅ |
| Trade portability across hosts (export → import → identical signing identity) | ❌ | ✅ |
| Existing machine-key flow when no AgentIdentity is loaded | ✅ | ✅ (unchanged) |

### Tests

5 new unit tests on `resolve_outbound_trade_signing`, all green:
- `machine_path_uses_machine_node_id_and_callback_signer`
- `agent_path_uses_agent_pubkey_as_provider`
- `agent_path_produces_valid_ed25519_signature`
- `two_agents_sign_the_same_canonical_with_different_signatures`
- `signed_trade_record_verifies_when_provider_signed_by_agent_identity`
  — full end-to-end shape: a SignedTradeRecord with the agent
  pubkey as provider AND the agent's Ed25519 signature passes
  `SignedTradeRecord::verify()` through the existing ledger
  verifier, with no Wave-2-specific awareness on the verifier
  side.

Workspace: **1,354 passed, 0 failed** (was 1,349 → +5 new).

The 5-test count is intentionally on the smaller side because the
unit tests pin the precise contract (provider attribution +
signature correctness) and the existing 1349-test suite catches
any regression in the P2P pipeline integration. A full end-to-end
"two real iroh peers + identity load + trade roundtrip" test
would require ~minutes of test setup and is deferred until Wave
2.5 lands AppState↔TiramiNode handle-sharing (so the HTTP layer
can drive the same identity slot the pipeline reads).

### Wave 2.5 — shared agent_identity Arc ✅ shipped

Wave 2 left an awkward decoupling: `AppState.agent_identity` and
`TiramiNode.agent_identity` were two **disjoint** instances of
`Arc<Mutex<Option<AgentIdentity>>>`. HTTP `agent/identity/init`
populated AppState's slot only — the pipeline kept reading the
TiramiNode slot, which remained `None` forever. Wave 2's signing
logic was correct but unreachable.

Wave 2.5 unifies them:

- **`create_router_with_services` gained an `agent_identity:
  Arc<Mutex<Option<AgentIdentity>>>` parameter.** AppState now
  stores the supplied handle instead of constructing its own. All
  6 call sites updated (the thin `create_router` wrapper, the
  two `TiramiNode` API entrypoints, the test helpers, and
  `security_tests.rs`).
- **`TiramiNode::serve_api` and `serve_api_with_listener` pass
  `self.agent_identity.clone()`** — the SAME Arc the pipeline
  receives through `run_seed`. HTTP-side mutations are now
  immediately visible to the signing path.
- **Test helpers default to a fresh, unshared Arc** so the 1,354
  pre-Wave-2.5 tests stay unchanged.

### Properties guaranteed

| Property | Pre-Wave-2.5 | Post-Wave-2.5 |
|---|---|---|
| HTTP `init_identity` updates AppState's agent_identity slot | ✅ | ✅ |
| Same call updates the slot the pipeline reads | ❌ (disjoint Arc) | ✅ (shared Arc) |
| Cross-node import propagates to pipeline | ❌ | ✅ |
| Pre-Wave-2.5 callers (tests, simple `create_router`) still work | ✅ | ✅ (unshared default) |

### Tests

2 new integration tests, all green:
- `shared_agent_identity_arc_receives_init` — caller supplies an
  externally-held Arc, hits `POST /v1/tirami/agent/identity/init`
  over HTTP, then re-reads the external Arc and confirms the DID
  matches the HTTP response.
- `shared_agent_identity_arc_replaced_on_import` — same shape but
  for the `/import` path, including the export-from-A → import-on-B
  cross-node round-trip.

Workspace: **1,356 passed, 0 failed** (was 1,354 → +2 new).

### Live smoke

`tirami start --port 3018`:

  [1] pre-init  /v1/tirami/agent/status        → wallet_source: machine_node, wallet: 30156cf7…
  [2] POST /v1/tirami/agent/identity/init      → did:tirami:47b4a62b…
  [3] post-init /.well-known/tirami-agent.json → agent_did: did:tirami:47b4a62b…
  [3] post-init /v1/tirami/agent/status        → wallet_source: agent_identity, wallet: 47b4a62b…
                                                  (matches DID pubkey byte-for-byte)

The fact that `manifest.agent_did` and `status.wallet` are
*consistent* after init — and would be consistent for the
pipeline-side signer if there were P2P traffic to observe — is
the property Wave 2.5 makes structural rather than coincidental.

## Wave 3 — AgentIdentity on-disk persistence + auto-load ✅ shipped

Until Wave 3, an imported AgentIdentity lived only in memory — a
restart dropped it and the operator had to re-import from a
bundle. Wave 3 adds optional encrypted persistence so the
identity survives restarts as cleanly as the rest of the ledger
state.

### Design

Wave 4 already shipped the perfect on-disk format: the
`AgentIdentityBundle` is JSON-serialisable, encrypted with
Argon2id + XChaCha20-Poly1305, and authenticates the salted
ciphertext. Wave 3 reuses **exactly that envelope** as the
at-rest format — an exported bundle and a persisted-on-disk file
are byte-identical structures. An operator can roll their own
backup strategy (rsync, B2, etc.) without diverging on schema.

### What changed

- **`state_persist::save_agent_identity(&id, path, passphrase)`** —
  writes the encrypted bundle. On Unix, sets `chmod 600` after
  the write for defense-in-depth (the file is already encrypted;
  the perm bit just blocks accidental multi-user reads on shared
  hosts).
- **`state_persist::load_agent_identity(path, passphrase)`** —
  returns `Ok(None)` when the file is absent (clean first-boot),
  `Ok(Some(_))` on success, `Err(_)` on I/O / JSON / passphrase
  failures.
- **`Config.agent_identity_path: Option<PathBuf>`** and
  **`Config.agent_identity_passphrase_env: String`** (default
  `"TIRAMI_AGENT_IDENTITY_PASSPHRASE"`). Both must be set for
  persistence to engage. Either alone is a silent no-op — the
  identity stays ephemeral.
- **`TiramiNode::new` auto-loads** at startup. A wrong passphrase
  logs at `warn` but does NOT abort: the node still boots with
  a fresh in-memory-only identity slot.
- **HTTP `init_identity` and `import_identity` auto-persist** —
  after the AppState slot is populated, the same encrypted
  bundle is written to disk. Best-effort: a write failure logs
  at `warn` but the HTTP request still returns the in-memory
  identity normally.

### Operator UX

To enable persistence:

```bash
export TIRAMI_AGENT_IDENTITY_PASSPHRASE="$(openssl rand -hex 32)"
# in the config file or via CLI:
agent_identity_path = "~/.tirami/agent_identity.json"
```

Without the env var, the path is ignored. Without the path, the
env var is ignored. This means a misconfigured deploy can't
silently store unencrypted material — the only fail-open is
"don't persist", never "persist in plaintext".

### Tests

7 new unit tests on `state_persist`:
- `agent_identity_save_then_load_round_trip`
- `agent_identity_load_missing_path_returns_none`
- `agent_identity_load_wrong_passphrase_returns_err`
- `agent_identity_save_rejects_short_passphrase`
- `agent_identity_save_corruption_then_load_returns_err`
- `agent_identity_save_file_has_restrictive_permissions_on_unix`
- `agent_identity_two_round_trips_preserve_seed_byte_for_byte`

Workspace: **1,363 passed, 0 failed** (was 1,356 → +7 new).

### Properties

| Property | Pre-Wave-3 | Post-Wave-3 |
|---|---|---|
| AgentIdentity survives node restart | ❌ | ✅ (when configured) |
| Persisted file is encrypted at rest | N/A | ✅ (Argon2id + XChaCha20-Poly1305) |
| Wrong passphrase aborts startup | N/A | ❌ (warn + fall back to ephemeral) |
| Missing env var silently disables persist | N/A | ✅ |
| File mode 0600 on Unix | N/A | ✅ |

## Phase 24 — zkML real backends

Now the only 🟡 remaining from Phase 20-23. Bound by:
- `ezkl` workspace dep (with feature flags for native vs WASM proving)
- bridging `tirami-zkml-bench`'s `MockBackend` to a real backend
  behind a runtime selector
- per-model benchmark calibration so proof time stays bounded
- governance ratchet wiring so the proof requirement can step
  up from Optional → Recommended → Required without breaking
  legacy clients

Multi-week scope and deliberately reserved for its own phase.

## Out of Phase 23 scope (Phase 24+)

- **zkML real backends**. `ezkl` + `risc0` integration is a
  multi-week project that warrants its own Phase. Scope:
  - workspace deps for `ezkl` (with feature flags for native vs
    WASM proving)
  - bridge `tirami-zkml-bench`'s `MockBackend` to a real backend
    behind a runtime selector
  - per-model benchmark calibration so proof time stays bounded
  - governance ratchet wiring so the proof requirement can step
    up from Optional → Recommended → Required without breaking
    legacy clients
- **NIP-39 binding** between the secp256k1 Nostr pubkey and a
  `did:tirami:` identity, so an agent's Nostr presence is
  cryptographically tied to its Tirami identity.
