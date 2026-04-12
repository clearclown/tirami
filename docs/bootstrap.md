# Forge — Bootstrap Sequence

## Overview

Forge has two bootstrap paths:

- the **current reference flow**, which is explicit and operator-driven
- the **target flow**, where a mesh-llm-based node joins a mesh and starts earning TRM automatically

## Current Reference Flow

```text
1. Start a model host: forge seed -m "qwen2.5:0.5b"
2. Copy the printed public key
3. Connect a requester: forge worker --seed <seed_public_key>
4. Check status: forge status --url http://127.0.0.1:3000
5. Check TRM balance: curl http://127.0.0.1:3000/v1/tirami/balance
```

The HTTP API binds to `127.0.0.1` by default. If exposed, set `--api-token`.

## Target Bootstrap (mesh-llm fork)

Once Forge integrates with mesh-llm:

```text
1. forge --auto                          # join best public mesh
2. forge --model Qwen2.5-32B --publish   # create public mesh, earn CU
3. forge --join <token>                  # join with GPU, earn CU
4. forge --client --join <token>         # join as consumer, spend CU
```

Every inference served earns CU. Every inference consumed spends CU. The economic layer is automatic — no separate configuration needed.

## Economic Bootstrap

### New Node (Zero Balance)

```text
1. Node joins mesh
2. Free tier: 1,000 TRM available immediately
3. Node serves first inference request → earns CU
4. TRM balance grows with each request served
5. Node can now spend TRM on other nodes' inference
```

### Existing Node (Has Balance)

```text
1. Node restarts, loads persisted ledger (tirami-ledger.json)
2. HMAC-SHA256 integrity verified
3. Previous balance, trades, and reputation restored
4. Node resumes earning and spending CU
```

## Degradation & Recovery

| Event | Economic Impact | Inference Impact |
|---|---|---|
| 1 remote node disconnects | Remaining nodes absorb work, TRM flow continues | Brief pause, model rebalanced |
| All remote nodes disconnect | TRM economy pauses, local-only mode | Fall back to local small model |
| Node low battery (<20%) | Stop serving (earning pauses), can still consume | Offload layers to remote |
| Node regains network | Resume earning CU, reputation recovers | Re-discover peers, re-expand |

**Key invariant**: A node's TRM balance persists across restarts and disconnections. Earned TRM is never lost.

## Node Contribution Model

- **Contributors**: Devices serving inference earn CU
- **Consumers**: Devices requesting inference spend CU
- **Balance**: More contribution → more TRM → more access to others' compute
- **Free tier**: 1,000 TRM for new nodes, consumed from first request
- **Yield**: Online nodes earn 0.1% yield per hour (reputation-weighted)
- **No mandatory payment**: The protocol runs on CU. External bridges (Lightning, fiat) are optional.

## Security During Bootstrap

- Ed25519 identity created before any network activity
- All connections encrypted via QUIC + Noise
- In the current seed/worker flow, the seed sees prompt text (explicit trust boundary)
- TRM trades are recorded locally with HMAC-SHA256 integrity
- Target: dual-signed trades gossip-synced across the mesh
