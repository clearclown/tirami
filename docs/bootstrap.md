# Forge — Bootstrap Sequence

## Overview

Forge has two bootstrap stories:

- the **current reference flow**, which is explicit and operator-driven
- the **target flow**, where a local-first client expands into split inference across additional peers

The current repository implements the first one.

## Current Reference Flow

The repo today exposes the bootstrap path through the daemon and CLI:

```text
1. Start a model host: `forged seed -m model.gguf -t tokenizer.json`
2. Copy the printed public key
3. Connect a requester: `forge worker --seed <seed_public_key>`
4. Inspect runtime state: `forge status --url http://127.0.0.1:3000`
```

This keeps the operator path explicit while the protocol surface remains small.

## Target Bootstrap (planned)

The intended bootstrap path for future clients is still:

1. local model works first
2. LAN peers are discovered and evaluated
3. layers are assigned across a small trusted cluster
4. WAN expansion happens only after the split runtime is stable
5. the system degrades back toward local execution as peers leave

That remains the target, not the current reference bootstrap.

## Degradation & Recovery (design target)

These are target properties for split inference, not guarantees of the current reference CLI flow.

| Event | Response | User Impact |
|---|---|---|
| 1 remote node disconnects | Rebalance remaining nodes, possibly downgrade model | Brief pause, then continue |
| All remote nodes disconnect | Fall back to local 1.5B model instantly | Quality drops, but chat continues |
| Phone loses network | 100% local mode | Same as above |
| Phone low battery (<20%) | Offload all layers to remote, phone only does tokenization | Reduced battery drain |
| Phone regains network | Re-discover peers, re-expand | Quality improves again |

**Key invariant:** the coordinator should always retain a viable local execution path before it starts delegating layers outward.

## Node Contribution Model

Forge uses a reciprocity model similar to BitTorrent:

- **Contributors**: Devices running `forged seed` donate idle compute
- **Consumers**: Devices requesting inference consume compute
- **Balance**: Nodes that contribute more get priority access to other nodes' compute
- **Free tier**: New nodes with no contribution history can still use the network, but with lower priority
- **No payment required**: The protocol works on mutual benefit first; any crypto/fiat payout is an optional adapter layered on top

This creates a natural flywheel only after split inference exists in the runtime.

## Bootstrap Relay Servers

Minimal infrastructure required to seed the network:

- 2-3 Iroh relay nodes on cheap VPS instances
- Purpose: help nodes find each other when DHT is sparse
- Do NOT carry inference traffic (zero knowledge of prompts/responses)
- Can be run by anyone (open source relay software)
- Network should function without them once direct discovery and peer learning are mature enough

## Security During Bootstrap

- Ed25519 identity created before any network activity
- First network message is already inside a Noise-encrypted QUIC tunnel
- Relay servers see only encrypted packets (connection metadata, not content)
- Peer discovery reveals only: node ID, capabilities, region — never prompts or responses
- In the current seed/worker flow, the seed sees prompt text because it runs the full model
- In the target split-inference flow, middle-stage peers should see only activation tensors for their assigned layers
