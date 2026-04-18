# Tirami Public API Surface

> Phase 18.4 · 2026-04-18.
> The 5-crate public surface for Filecoin-path credibility.

One of the critiques that distinguishes Tirami from Bitcoin is
size: Bitcoin's 2009 core was ~16 kLoC across a flat file
structure, fully understandable by one engineer in a week. Tirami
is 15 workspace crates / ~30 kLoC — already larger than Bitcoin
ever needs to be.

We cannot reduce the code itself (security hardening requires
breadth), but we CAN reduce the **public API surface** that
external readers / integrators / auditors need to understand.
Everything below is the canonical set of public crates. Anything
else is internal, subject to change without notice, and
`#[doc(hidden)]` where appropriate.

---

## The five public crates

### 1. `tirami-core` — types & invariants

What it does: the economic type system. `NodeId`, `TRM`, `Config`,
`TradeRecord`, `HybridSignature`, `NodeIdentity`,
`AttestationReport`.

When you depend on it: any code that reads/writes Tirami data
structures. SDKs, bridges, tools, alternative node
implementations.

Stability promise: semver-major on breaking changes.
Constitutional parameters are frozen.

### 2. `tirami-ledger` — economic engine

What it does: `ComputeLedger`, trades, lending, staking,
slashing, audit tracker, checkpoints, fork detection,
sybil limiter, governance.

When you depend on it: running a Tirami node, building a
block explorer, writing analytics.

Stability promise: semver-major on API breaks. Per-trade
behaviour guarded by the Constitution (see `docs/constitution.md`).

### 3. `tirami-node` — daemon

What it does: HTTP API (`/v1/chat/completions`,
`/v1/tirami/*`), P2P pipeline, scoped API tokens, the actual
`TiramiNode::run_seed` loop that ties everything together.

When you depend on it: running a node. This is the single
binary operators use.

Stability promise: HTTP API paths are semver-stable.
Internal pipeline internals may change.

### 4. `tirami-infer` — inference trait

What it does: `InferenceEngine` trait, `CandleEngine` /
`LlamaCppEngine` implementations, `generate_audit_at_layer`
for SPoRA.

When you depend on it: writing a custom inference backend.
When you do NOT: just running the reference node — that
embeds `CandleEngine` already.

Stability promise: trait shape semver-stable; implementation
choices are internal.

### 5. `tirami-contracts` — on-chain (Foundry)

What it does: `TRM.sol` ERC-20 + `TiramiBridge.sol`. Deployed
to Base L2 post-audit.

When you depend on it: writing a Tirami-aware DeFi integration
(DEX, lending protocol, etc.). Never imported as a Rust
dependency.

Stability promise: `TRM` ERC-20 interface is eternal. Bridge
functions are governable but the Constitutional parameters
(21 B cap, mint cooldown) are immutable.

---

## Internal crates (subject to change; not public API)

These are `pub` at the workspace level for convenience and for
integration tests, but they are NOT public API. External callers
should depend on them only with the understanding that **breaking
changes may land in any patch release**.

| Crate | Reason internal |
|-------|-----------------|
| `tirami-net` | iroh transport; replaced if upstream evolves |
| `tirami-proto` | wire messages; versioned internally |
| `tirami-shard` | topology planning; alternative planners welcome |
| `tirami-cli` | reference CLI; UX may change |
| `tirami-lightning` | Lightning bridge; experimental |
| `tirami-anchor` | Base L2 client; implementation-detail |
| `tirami-bank` | L2 financial strategies (experimental) |
| `tirami-mind` | L3 self-improvement harness (experimental) |
| `tirami-agora` | L4 marketplace (experimental) |
| `tirami-sdk` | Python-facing wrapper |
| `tirami-mcp` | MCP server for Claude Code / Cursor |
| `tirami-zkml-bench` | Phase 18.3 benchmark harness; internal tool |

## Versioning policy

- The 5 public crates release in lock-step at the workspace
  `version.workspace = "X.Y.Z"`.
- Breaking changes to a public crate require a semver-major
  bump of the whole workspace.
- Internal crates can have their API rearranged within a patch
  release if no public-crate signature changes.

## How to depend on Tirami

```toml
# Minimum: data types + ledger
[dependencies]
tirami-core = "0.3"
tirami-ledger = "0.3"

# Running a node (includes everything above)
[dependencies]
tirami-node = "0.3"
tirami-infer = "0.3"

# Writing a DeFi integration (on-chain)
# (Solidity dependency, not Cargo)
# Via: forge install clearclown/tirami-contracts
```

## Audit position

An auditor only needs deep review of the 5 public crates. The
internal crates are in-scope from the standpoint of "does the
public surface hold up" but their internal design is a
refactoring target, not a stability contract.

Priority order for audit review (from `docs/security/audit-scope.md`):

1. `tirami-ledger` — economic invariants live here.
2. `tirami-node/src/api.rs` — HTTP attack surface.
3. `tirami-net/src/gossip.rs` — P2P replay / dedup (internal
   crate, but security-critical).
4. `tirami-proto` — wire-format validation (internal crate
   but the boundary between trusted and untrusted bytes).
5. `tirami-anchor` — chain-facing serialization.
6. `tirami-core/src/crypto.rs` — HybridSignature scaffold.
