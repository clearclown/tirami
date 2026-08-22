# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Tirami is a distributed LLM inference protocol whose original contribution is
the **economic layer**: TRM accounting (1 TRM = 10⁹ FLOP), dual-signed
bilateral trades, dynamic pricing, lending, staking, governance, and agent
budgets. The inference layer (llama.cpp + iroh QUIC) is derived from mesh-llm
and is the part most likely to be replaced.

Rust edition 2024, resolver v2, 17 workspace crates under `crates/`.

**The README's "⚠️ Status Honesty" section is authoritative** for what works
today vs. what is scaffolded vs. what is not started. Keep this file in sync
with it, never ahead of it. `docs/pmvv.md` holds the purpose, the three-stage
honesty about capability, and the phrasing rules the docs follow.

## Build and test

```bash
cargo check --workspace                    # fast type check
cargo test --workspace                     # full suite
cargo clippy --workspace                   # lint (CI gate; --all-targets has pre-existing noise)
./scripts/verify-impl.sh                   # conformance suite — 123 assertions mapping theory → code
```

`verify-impl.sh` is the real gate. It greps for specific symbols, endpoints and
constants and ends with `cargo check` + `cargo test`, so it catches "the
constant moved but the doc didn't" drift that unit tests miss.

Single crate, single test, single integration file:

```bash
cargo test -p tirami-ledger                          # one crate
cargo test -p tirami-ledger sybil                    # name filter
cargo test -p tirami-net --test rpc_tunnel           # one integration file
cargo test -p tirami-node --lib topology             # unit tests in one module
```

Some tests need real resources and skip rather than fail without them:

- Socket-binding P2P tests skip when the sandbox denies `bind` — see
  `transport_or_skip` in `crates/tirami-net/tests/p2p_connection.rs`. Follow
  that pattern for anything needing a real endpoint.
- Tests that need a large model read a path from an env var and skip when it
  is unset, rather than fabricating a manifest.

Test counts drift. Get the current number from `cargo test --workspace` rather
than quoting a doc — several docs have carried stale counts for months.

## GPU builds

Backends are compiled into llama.cpp at build time. **Name the binary crate**;
the workspace root declares no features, so a bare `cargo build --features
cuda` fails with "none of the selected packages contains these features":

```bash
cargo build --release -p tirami-cli --features cuda    # NVIDIA
cargo build --release -p tirami-cli --features metal   # Apple Silicon
```

`tirami-cli` and `tirami-node` relay `metal` / `cuda` / `rocm` / `vulkan` down
to `tirami-infer`, which relays to `llama-cpp-2`. No Rust code is conditionally
compiled on them. On macOS `llama-cpp-sys-2` enables Metal from its own build
script regardless, so `default = []` still yields a Metal binary there.

A CUDA build still needs `TIRAMI_GPU_LAYERS` (default 256) to actually place
layers on the device; without it the binary runs on the CPU and looks healthy.

## Architecture

Two layers, and the split matters when deciding where a change belongs.

```
Economic layer (this project's contribution)
  tirami-ledger    TRM balances, trades, pricing, yield, lending, collusion, slashing
  tirami-node      daemon, HTTP API, pipeline coordinator, split-inference orchestrator
  tirami-anchor    Merkle-root anchoring to chain (MockChainClient by default)
  tirami-bank / -mind / -agora     L2 finance / L3 self-improvement / L4 marketplace
  tirami-lightning CU↔BTC bridge (optional, never a hard dependency)

Inference layer (mesh-llm-derived)
  tirami-net       iroh QUIC + Noise, gossip, ALPN-separated RPC tunnel
  tirami-infer     llama.cpp engine, rpc-server subprocess manager, distributed wrapper
  tirami-proto     wire protocol (bincode)
  tirami-shard     layer assignment / peer selection
```

`repos/` holds in-tree sibling repos: `tirami-contracts` (Foundry — TRM ERC-20
+ TiramiBridge, not deployed to mainnet) and `tirami-economics` (the theory
spec that L2/L3/L4 numeric constants reference rather than redefine).

### The seed recv loop is a single consumer

`PipelineCoordinator::run_seed` is the only consumer of `transport.recv()`.
Anything that needs a reply routed back to a waiting task must register a
oneshot in a dispatcher map that the loop resolves — see
`TradeAcceptDispatcher` and `RpcReadyDispatcher`. Adding a second consumer of
`recv()` will silently steal messages.

The same applies at the stream level: `read_peer_messages` consumes **every**
inbound bidirectional stream on a peer connection and decodes it as an
`Envelope`. That is why the llama.cpp RPC tunnel runs on its own ALPN
(`RPC_TUNNEL_ALPN`) and therefore its own QUIC connection.

### Wire-format constraints (bincode)

`Payload` is serialized with bincode, which is **not self-describing**:

- Enum variants are encoded by **index**. Append new variants at the end;
  inserting one renumbers every variant after it and breaks the wire for all
  of them. `stop_rpc_server_is_the_last_payload_variant` pins this.
- Adding a struct field is a wire break. `#[serde(default)]` does **not**
  rescue an older peer — it only helps self-describing formats.
- 64 MiB cap enforced on receive (`MAX_PROTOCOL_MESSAGE_BYTES`).

`PeerCapability` rides inside `Hello` / `Welcome` and is in live use, so extend
it via the existing `features: Vec<String>` rather than by adding fields.

## Configuration

`Config` (`crates/tirami-core/src/config.rs`) derives Serialize/Deserialize
with a container-level `#[serde(default)]`, so every field is optional.
Resolution order is CLI flags → `<data-dir>/config.toml` → `Config::default()`.

Two deliberate behaviours in the loader: unrecognised keys are logged rather
than dropped (a misspelled key is otherwise indistinguishable from an absent
one), and a malformed file is a hard error rather than a silent fallback.

Adding a field means updating `KNOWN_FIELDS` in the same file — the
`known_fields_matches_struct` test fails otherwise.

## Working with llama.cpp

Facts established by running it, not from documentation:

- The RPC binary is **`ggml-rpc-server`** in current builds; `rpc-server` no
  longer exists. Both names are searched.
- **`-d <device>` is required, not an optimisation.** Without it the server
  selects BLAS and aborts on the first graph with `unsupported op RMS_NORM`.
  Defaults to `MTL0` / `CUDA0`; `TIRAMI_RPC_DEVICE=auto` opts out.
- `llama-cli` prints a terminal UI to stdout regardless of `-no-cnv`,
  `--no-display-prompt`, or `--simple-io`, and needs `-st` or it blocks on
  stdin. The completion is bracketed out in `extract_generated_text`.
- Per-device load lines only appear at `-v`. Without it stderr is empty.
- `ggml-rpc` returns `free = 0, total = 0` instead of an error when it cannot
  reach a server, so llama.cpp assigns it no layers and inference **succeeds on
  one machine**. `verify_layers_distributed` exists to catch that.

Test fixtures in `crates/tirami-infer/tests/fixtures/` are captured output from
real runs, not hand-written approximations.

## Design rules

1. **TRM is the native unit.** Bitcoin/Lightning is an optional off-ramp, never
   a hard dependency of the economic engine.
2. **Trades and loans are bilateral.** Every transfer has a provider and a
   consumer, and both sign. No unilateral issuance.
3. **Settlement exports, it does not pay.** `/settlement` produces data;
   external bridges execute.
4. **Local-first reputation and credit.** Each node computes from its own
   observed history. No central bureau.
5. **Lending fails safe.** Pool reserve floor, velocity limits, default-rate
   triggers; if a check cannot determine safety, deny.
6. **Agent-first API.** `/v1/tirami/balance` and `/pricing` exist so an agent
   can decide without a human.

Two rules previously listed here — "no blockchain in the core" and "no tokens,
no ICO" — are **under active reconsideration** (issues #174–#181). Do not treat
either as settled, and do not add public claims that depend on them without
checking those issues first.

## Conventions

- Errors: `TiramiError` in library crates, `anyhow` in the CLI only.
- Serialization: `serde`/`serde_json` for config and HTTP, `bincode` for the
  wire, `toml` for the operator config file.
- Async: `tokio`, `Arc<Mutex<T>>` for shared state. Blocking work
  (`std::thread::sleep`, subprocess waits) goes in `spawn_blocking`.
- Logging: `tracing`. Steady-state economic verdicts belong at DEBUG — a
  decision the protocol is designed to make is not a warning, and at WARN its
  volume scales with retry rate (#150, #153).
- Two binaries from `tirami-cli`: `tirami` and `tiramisu` (daemon). They share
  no module, so resolution helpers are duplicated in both.

## Common tasks

**New economic endpoint** — handler in `crates/tirami-node/src/api.rs`, wire
into the `protected` router in `create_router_with_services`, add a test in the
same file's `#[cfg(test)]` block. Note that constructor already takes 21
arguments; new shared state usually belongs in a small struct rather than a
new parameter.

**Ledger change** — `crates/tirami-ledger/src/ledger.rs`, test in the same
file's `mod tests`. New fields on `NodeBalance` or `TradeRecord` also touch
`crates/tirami-core/src/types.rs`.

**New wire message** — add the variant at the **end** of `Payload`
(`crates/tirami-proto/src/messages.rs`), add validation in
`validate_with_sender`, handle it in `crates/tirami-node/src/pipeline.rs`.
