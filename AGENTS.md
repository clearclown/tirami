# AGENTS.md — Forge

## Project

Forge is a self-expanding LLM protocol over encrypted P2P networks.

A small LLM on a phone discovers idle devices, shards itself across them via pipeline parallelism, and grows autonomously. All traffic encrypted. All compute logically local.

**Tagline:** "A seed falls into the network and grows into a forest."

## Workspace Structure

Rust monorepo with Cargo workspaces:

```
crates/
  forge-core/    — Shared types (NodeId, ShardId, ModelManifest, Config, errors)
  forge-net/     — P2P networking via Iroh (QUIC, Noise, NAT traversal, mDNS)
  forge-shard/   — Model layer partitioning, assignment, rebalancing
  forge-infer/   — Inference engine (Candle + GGUF + Metal/CPU)
  forge-proto/   — Wire protocol message types (serde + bincode)
  forge-ledger/  — Compute economy (CU, trades, yield, reputation)
  forge-node/    — Node daemon, orchestrator, event loop
  forge-cli/     — Reference CLI client (chat, seed, worker, status)
```

## Key Technical Invariants

- All network traffic MUST be encrypted (Noise protocol over QUIC)
- Pipeline parallelism only over WAN (tensor parallelism only for LAN/Thunderbolt)
- Phone always holds layers 0..k (embedding + early layers) for instant fallback
- Graceful degradation: if all remote nodes disconnect, fall back to local model
- GGUF Q4 is the model format — no custom formats
- Activation tensors use raw bytes + optional int8 quantization for WAN transfer

## What NOT to Do

- Do NOT add blockchain or smart contracts
- Do NOT add centralized servers (except bootstrap relays)
- Do NOT send unencrypted data over the network
- Do NOT use tensor parallelism over WAN (physics won't allow it)
- Do NOT add GPU/CUDA support in MVP (Apple Silicon Metal + CPU only)
- Do NOT use protobuf for tensor payloads (raw bytes only)
- Do NOT build UI — Forge is a protocol, clients are built by third parties
- Do NOT add mobile-specific code (UniFFI, SwiftUI, Compose)

## Testing Strategy

- `forge-core`: Unit tests for type serialization/deserialization
- `forge-infer`: Integration tests with small GGUF models (Llama-1B-Q4)
- `forge-net`: Integration tests with local Iroh nodes
- `forge-shard`: Unit tests for layer assignment algorithms
- `forge-node`: Multi-process integration tests (2+ nodes on localhost)
- `forge-cli`: Smoke tests for CLI commands

## Docs

- `docs/concept.md` — Why Forge exists
- `docs/economy.md` — Compute Standard, CU, trades, yield
- `docs/architecture.md` — Technical architecture
- `docs/protocol-spec.md` — Wire protocol specification
- `docs/bootstrap.md` — How a node starts and grows
- `docs/threat-model.md` — Security considerations
- `docs/roadmap.md` — Implementation phases
