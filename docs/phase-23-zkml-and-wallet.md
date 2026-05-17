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

## Wave 2 — TBD

Candidates:

1. **Migrate the P2P trade signer to AgentIdentity.** The remaining
   half of the wallet/identity link. When an `AgentIdentity` is
   loaded, outbound `SignedTradeRecord` instances should be signed
   with the agent's `SigningKey`, not the machine's. Touches
   `tirami-net` signing paths and the pipeline coordinator. Bounded
   scope (~one PR) but invasive across crate boundaries.
2. **AgentIdentity on-disk persistence + auto-load**. Today the
   imported identity lives only in memory; a restart drops it.
   Adding a snapshot path that survives restart closes another
   portability footgun.

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
