# Forge — Roadmap

Forge is a protocol. Like Bitcoin Core, we ship the daemon (`forged`), the CLI, and the spec. Clients and integrations are built by the community.

## Phase 1: Local Inference ✅

**Goal:** Single-node GGUF inference in Rust.

- `forge-core`: Type system (NodeId, LayerRange, ModelManifest, PeerCapability)
- `forge-infer`: Candle engine, GGUF loader, streaming token generation
- `forge-node`: HTTP API (/chat, /chat/stream, /health)
- `forge-cli`: `forge chat` command

## Phase 2: P2P Protocol ✅

**Goal:** Two nodes communicate over encrypted QUIC.

- `forge-net`: Iroh transport, Noise encryption, peer connections
- `forge-proto`: 14 wire protocol message types (bincode + length-prefix)
- `forge-node`: Seed/Worker pipeline, inference request/response
- Integration tests: 2 nodes exchange Hello + multiple messages

## Phase 3: Remote Inference + Operator Ledger ✅

**Goal:** Encrypted seed/worker inference with CU-native accounting.

- `forge-ledger`: Compute Units, trade execution, reputation, yield, market pricing
- `forge-node`: Ledger integrated into inference pipeline
- CU balance checks before inference
- Trade records after completion

## Phase 4: Split Inference MVP (current)

**Goal:** Close the gap between the story and the runtime.

| Deliverable | Description |
|---|---|
| Honest docs/spec | Distinguish current seed/worker flow from planned split inference |
| Capability handshake | Exchange peer capabilities and retain them in runtime state |
| Topology planning | Build a shard plan from GGUF metadata + connected peers |
| Partial layer load | `forge-infer` actually respects `LayerRange` |
| Two-node activation path | `Forward` messages execute a real 2-stage inference path |
| Runtime topology wiring | `forge-shard` output drives actual execution |
| Explicit trust modes | Separate trusted-LAN and remote-provider modes in scheduling |
| Split-inference tests | End-to-end tests for 2-node inference and failure handling |

## Phase 5: Hardening + Network Growth

**Goal:** Multi-node WAN operation.

| Deliverable | Description |
|---|---|
| Daemon/CLI split | `forged` for long-running node operation, `forge` for operator/client actions |
| Runtime observability | `/status` endpoint, recent trade visibility, market price inspection |
| Protocol versioning | Version negotiation in Hello/Welcome |
| Graceful reconnection | Resume sessions after transient disconnects |
| Heartbeat failure detection | 10s timeout → mark node as down |
| Dynamic rebalancing | Redistribute work when nodes join/leave |
| Bootstrap relay | Iroh relay on VPS for initial peer finding |
| DHT discovery | Mainline DHT for WAN peer advertisement |
| mDNS discovery | Same-LAN peer discovery without explicit address sharing |
| Multi-seed topology | Multiple seed nodes sharing inference load |
| Bandwidth optimization | INT8 activation tensor quantization for WAN |
| Settlement exports | Signed CU statements for dashboards, billing, and payout adapters |

## Phase 6: Market + Scheduling

**Goal:** Make the network economically and operationally robust.

| Deliverable | Description |
|---|---|
| Reputation propagation | Gossip-based reputation sharing between nodes |
| Reserved CU windows | Hold and settle spend budgets for in-flight inference |
| Auto model selection | Choose best model given available compute |
| Speculative pipeline | Pre-compute while waiting for upstream |
| KV cache distribution | Shared cache across the network |
| Self-healing topology | Automatic recovery from any failure mode |

## Phase 7: Agent Integration

**Goal:** Let software agents consume Forge without changing the protocol boundary.

| Deliverable | Description |
|---|---|
| Agent integration | MCP/A2A tool for AI agents to use Forge |
| Budget APIs | Safe spend policies for automated callers |
| External payout adapters | Optional CU → credits / stablecoin / fiat integrations outside the protocol |

## Long-term

| Milestone | Description |
|---|---|
| SDK release | `forge-node` as embeddable Rust library with stable API |
| Protocol v2 | Lessons from v1, backward-compatible evolution |
| Cross-architecture | NVIDIA GPU, AMD ROCm, RISC-V support |
| Federated training | Distributed fine-tuning, not just inference |
| Compute derivatives | Forward contracts on future compute capacity |

> The protocol is the platform. Build what you want on top.
