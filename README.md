# Forge

> A seed falls into the network and grows into a forest.

**Open protocol for encrypted P2P inference, with split inference as the target architecture.**

Forge follows the Bitcoin Core shape: `forged` for the daemon, `forge` for the operator/client CLI, and `docs/protocol-spec.md` as the wire contract.

The current reference implementation is an encrypted seed/worker inference protocol over Iroh: a worker connects to a seed, sends a prompt over an authenticated encrypted channel, receives streamed text, and records CU-native trades locally. The long-term goal remains split inference by layer pipeline, but that is not the current runtime path yet.

## Project Status

- Current: encrypted remote inference, local HTTP API, local CU ledger, persisted ledger snapshots, settlement export
- Current groundwork for split inference: capability handshake, model metadata parsing, topology planning endpoint
- Next: partial-layer model loading, `Forward`-based activation routing, topology-driven split inference, honest fallback behavior
- Boundary: payout rails, credits, stablecoins, and fiat stay outside the protocol

## How It Works

```text
Current reference flow:
  1. Start `forged seed` on a machine with a GGUF model
  2. Connect with `forge worker --seed ...`
  3. Worker sends `InferenceRequest { prompt_text, ... }` over encrypted QUIC
  4. Seed runs the full model locally and streams `TokenStreamMsg { text, ... }`
  5. Local CU ledger records the completed trade

Target architecture:
  1. Coordinator keeps early layers locally
  2. Remote peers hold contiguous later layers
  3. `Forward` messages carry activation tensors between stages
  4. The system degrades gracefully as peers leave or reconnect
```

## Architecture

```
┌─────────────────────────────────────────────┐
│  SDK / Integration Layer                    │
│  Any client can embed forge-node as library │
└──────────────────┬──────────────────────────┘
┌──────────────────▼──────────────────────────┐
│  Orchestrator (forge-node)                  │
│  Local or distributed inference — automatic │
│  Compute Ledger — CU accounting & yield     │
└──────────────────┬──────────────────────────┘
         ┌─────────┼─────────┐
    ┌────▼───┐ ┌───▼───┐ ┌──▼────────┐
    │  P2P   │ │ Shard │ │ Inference │
    │  Iroh  │ │ Mgmt  │ │ Candle    │
    │  QUIC  │ │       │ │ GGUF      │
    │  Noise │ │       │ │ Metal/CPU │
    └────────┘ └───────┘ └───────────┘
```

## The Idea

Compute + Electricity = Value. A sleeping Mac Mini is an empty apartment — wasted potential.

Forge creates an open market where idle devices earn **Compute Units (CU)** by performing inference for others. Like Bitcoin miners earn BTC by hashing, Forge nodes earn CU by computing — except every joule produces *useful work* instead of meaningless hashes.

The protocol is the platform. Anyone can build clients, dashboards, settlement adapters, or integrations on top. Forge core itself stays small: `forged`, `forge`, and the spec.

## Project Structure

```
forge/
├── crates/
│   ├── forge-core/      # Shared types, config, errors
│   ├── forge-net/       # P2P networking (Iroh + QUIC + Noise)
│   ├── forge-shard/     # Model sharding and layer assignment
│   ├── forge-infer/     # Inference engine (Candle + GGUF)
│   ├── forge-proto/     # Wire protocol message definitions
│   ├── forge-ledger/    # Compute economy (CU, trades, yield)
│   ├── forge-node/      # Node daemon (orchestrator)
│   └── forge-cli/       # Reference CLI client
└── docs/
```

## Quick Start

```bash
# Build
cargo build --release

# Run local inference from the CLI
forge chat -m model.gguf -t tokenizer.json "What is gravity?"

# Start the daemon as a seed node (serves inference to the network)
forged seed -m model.gguf -t tokenizer.json --ledger forge-ledger.json

# Connect as a requester and buy inference from the seed
forge worker --seed <seed_public_key>

# Inspect daemon health, CU market price, and recent trades
forge status --url http://127.0.0.1:3000

# Inspect the current split-inference plan and any advertised remote topology
forge topology --url http://127.0.0.1:3000

# Export a 24h settlement statement with an external reference price
forge settle --url http://127.0.0.1:3000 --hours 24 --price 0.05 --out settlement-24h.json

# Local API mode without P2P
forged node -m model.gguf -t tokenizer.json --port 3000 --ledger forge-ledger.json
```

## Operator Flow

1. Run `forged seed` on the machine that will host the model.
2. Point consumers at it with `forge worker --seed ...`.
3. Keep `--ledger forge-ledger.json` enabled so balances and trades survive restarts.
4. Watch `/status` or `forge status` for market price, trade count, and CU flow.
5. Use `/topology` or `forge topology` to inspect the current shard plan from connected peer capabilities.
6. Export `/settlement` or use `forge settle` for off-protocol billing and payout adapters.
7. Build any payout or billing adapter outside the protocol boundary; the core ledger remains CU-native.

## Docs

- [Concept & Vision](docs/concept.md)
- [Economic Model](docs/economy.md) — Compute Standard, CU, yield, operator flows, payout boundary
- [Architecture](docs/architecture.md)
- [Wire Protocol](docs/protocol-spec.md)
- [Bootstrap Sequence](docs/bootstrap.md)
- [Threat Model](docs/threat-model.md)
- [Roadmap](docs/roadmap.md)

## Contributing

Forge is an open protocol. Build clients, integrations, dashboards — whatever you want on top. The reference implementation is here. The protocol spec is in `docs/protocol-spec.md`.

## License

MIT
