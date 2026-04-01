# Forge — Architecture

## Overview

Forge is a 5-layer protocol stack with a clear distinction between the current reference implementation and the target architecture.

Current reference implementation:
- encrypted seed/worker inference over Iroh
- full-model execution on the seed
- CU-native local accounting and settlement export
- capability handshake and topology planning from model metadata + connected peers

Target architecture:
- topology-driven split inference by contiguous layer ranges
- activation forwarding between peers
- graceful degradation back toward local execution

Optional billing, payout, and exchange integrations sit above the protocol rather than inside it.

```
┌─────────────────────────────────────────────────┐
│  SDK / Integration Boundary                     │
│  forge-node as embeddable Rust library          │
│  Third-party clients build on this API          │
└──────────────────┬──────────────────────────────┘
                   │ Rust crate API
┌──────────────────▼──────────────────────────────┐
│  Layer 5: Orchestrator (forge-node)             │
│  Manages local model shard                      │
│  Coordinates pipeline across peers              │
│  Decides: run locally or distribute?            │
└──────────────────┬──────────────────────────────┘
         ┌─────────┼──────────┐
┌────────▼───┐ ┌───▼────────────────────┐
│  Layer 4   │ │  Layer 3: Ledger       │
│  forge-net │ │  forge-ledger          │
│  P2P       │ │  Proof of Useful Work  │
│  Iroh/QUIC │ │  Compute accounting    │
│  Noise enc │ │  Reputation + Balance  │
└────────────┘ └────────────────────────┘
         ┌─────────┼──────────┐
┌────────▼───┐ ┌───▼────┐ ┌──▼──────────┐
│  Layer 2   │ │Layer 1b│ │  Layer 1a   │
│  forge-net │ │ shard  │ │ forge-infer │
│  (shared)  │ │ mgmt   │ │ Candle+GGUF │
│            │ │ assign │ │ Metal/CPU   │
│            │ │ rebal. │ │ KV cache    │
└────────────┘ └────────┘ └─────────────┘
```

## Layer 1: Inference Engine (forge-infer)

Loads GGUF model files and runs transformer inference. Partial-layer execution is the intended next step, but the current engine still loads and executes whole-model inference on one node.

**Responsibilities:**
- Load GGUF files and tokenizer state
- Run local text generation for the current seed/runtime flow
- Evolve toward partial-layer execution for split inference
- Use Metal on Apple Silicon when available, CPU fallback elsewhere

**Key interface:**
```rust
pub trait InferenceEngine: Send + Sync {
    fn load(
        &mut self,
        model_path: &Path,
        tokenizer_path: &Path,
        layer_range: Option<LayerRange>,
    ) -> Result<(), ForgeError>;

    fn is_loaded(&self) -> bool;

    fn generate(
        &mut self,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: Option<f64>,
    ) -> Result<Vec<String>, ForgeError>;
}
```

## Layer 2: Shard Management (forge-shard)

Decides how to split a model across available nodes.

**Responsibilities:**
- Parse GGUF metadata to determine layer structure
- Assign contiguous layer ranges to nodes based on their capabilities
- Rebalance when nodes join or leave
- Ensure the phone always holds layers 0..k (embedding + early layers)

**Assignment algorithm:**
```
1. Sort peers by compute_power descending
2. Calculate memory budget per peer
3. Greedily assign contiguous layer ranges
   - Phone gets layers 0..k (always)
   - Faster peers get more layers
4. If total memory < model size → try smaller quantization or smaller model
5. Return ShardPlan { assignments: Vec<(NodeId, LayerRange)> }
```

**Rebalancing triggers:**
- Node joins → steal layers from most-loaded peer
- Node leaves (heartbeat timeout 10s) → remaining peers absorb orphaned layers
- Insufficient memory → downgrade to smaller model (graceful degradation)

## Layer 3: Compute Ledger (forge-ledger)

Tracks the flow of compute value across the network. This is Forge's economic engine.

**Core principle:** Compute + Electricity = Value. Every forward pass a node executes is *useful work* — unlike Bitcoin's PoW, it directly produces intelligence.

**Current role:** local accounting for observed inference trades and optional contribution records.

**Target role:** account for split-inference work once the runtime really routes activations across nodes.

**Compute accounting:**
```rust
pub struct WorkUnit {
    pub node_id: NodeId,
    pub timestamp: u64,
    pub layers_computed: LayerRange,
    pub model_id: ModelId,
    pub tokens_processed: u64,
    pub estimated_flops: u64,
}

pub struct NodeBalance {
    pub node_id: NodeId,
    pub contributed: u64,    // total compute units contributed
    pub consumed: u64,       // total compute units consumed
    pub reserved: u64,       // budget held for in-flight work
    pub reputation: f64,     // 0.0 - 1.0, based on uptime and reliability
}
```

**Incentive design:**
- Nodes that contribute more compute earn higher balance
- Higher balance = priority access to the network's compute
- Nodes with zero balance can still use the network (free tier) but at lower priority
- The core protocol stays CU-only; any payout rail lives outside this layer
- Each node maintains its own local view of the ledger today

**MVP:** Off-chain local ledger per node. Each node records what it has observed.
**Future:** Signed settlement statements, stronger reconciliation, and optional exchange adapters run by third parties.

## Layer 4: P2P Networking (forge-net)

Encrypted peer-to-peer communication using Iroh.

**Transport:** QUIC over UDP
**Encryption:** Noise Protocol (ChaCha20-Poly1305) with forward secrecy
**Identity:** Ed25519 keypair per node, stored in platform keychain

**Discovery target:**

| Method | Scope | Latency | Use Case |
|---|---|---|---|
| mDNS (`_forge._udp.local`) | LAN | <1s | Same-network devices |
| DHT (Mainline) | WAN | 2-10s | Global device discovery |
| Bootstrap relays | WAN | 1-3s | Initial network entry |
| QUIC hole-punch | WAN | 1-5s | Direct NAT traversal |
| Relay fallback | WAN | 10-50ms overhead | When hole-punch fails |

Current reference implementation is narrower:
- direct seed address sharing
- connected-peer tracking
- capability exchange during handshake

**Peer capability advertisement:**
```rust
pub struct PeerCapability {
    pub node_id: NodeId,
    pub cpu_cores: u16,
    pub memory_gb: f32,
    pub metal_available: bool,
    pub bandwidth_mbps: f32,
    pub battery_pct: Option<u8>,  // None for plugged-in devices
    pub available_layers: u32,     // how many layers this node can hold
    pub region: String,
}
```

## Layer 5: Orchestrator (forge-node)

The main event loop that ties everything together.

**Modes:**
- **Seed mode**: Hosts a GGUF model, accepts encrypted inference requests, checks CU affordability, and streams text back
- **Worker / requester mode**: Connects to a seed, sends prompt text over the encrypted channel, and spends CU
- **Future hybrid mode**: A node both consumes and contributes layers in a multi-hop pipeline

**Current execution path:**
```
Worker sends `InferenceRequest { prompt_text, ... }`
  ↓
Seed checks CU affordability
  ↓
Seed runs whole-model generation locally
  ↓
Seed streams `TokenStreamMsg { text, is_final }`
  ↓
Ledger records the completed trade
```

**Target split-inference path:**
```
Coordinator keeps early layers local
  ↓
`Forward` carries activation tensors to the next stage
  ↓
Remote peers execute contiguous layer ranges
  ↓
Final logits or token results return to the coordinator
```

**Activation tensor size (Llama-7B target model):**
- FP16: `seq_len × 4096 × 2 bytes` = ~8KB per token position
- INT8 (WAN-optimized): ~4KB per token position

## SDK / Integration Boundary

Forge is a protocol, not an application. The `forge-node` crate is the integration point for third-party clients, but the current API surface still reflects a seed/worker remote inference runtime more than a full split-inference runtime.

**Public API surface:**
```rust
// Start a node
let node = ForgeNode::new(config);
node.load_model(&model_path, &tokenizer_path).await?;

// Local inference
let response = node.chat("What is gravity?", 256, 0.7).await?;

// P2P seed mode (serve inference to network, earn CU)
node.run_seed().await?;

// P2P worker mode (connect to seed, spend CU)
let transport = node.connect_to_seed(seed_addr).await?;

// Network statistics
let stats = node.network_stats().await;

// Local daemon status over HTTP
// GET /status -> model_loaded + market price + network stats + recent trades
// GET /topology -> model manifest + connected peer capabilities + planned shard topology
// GET /settlement -> export settlement statement for a time window
```

**Reference binaries:** `forged` (daemon) and `forge` (operator/client CLI)
**Protocol spec:** `docs/protocol-spec.md`
**Anyone can build:** desktop apps, web dashboards, mobile clients, agent integrations

## Data Flow (End to End)

```
Current implementation:
  Client sends prompt "What is gravity?"
    ↓
  forge worker → encrypted QUIC connection
    ↓
  forge-proto::InferenceRequest { prompt_text, max_tokens, ... }
    ↓
  Seed checks `can_afford`
    ↓
  forge-infer tokenizes prompt and generates text
    ↓
  forge-proto::TokenStreamMsg { text, is_final }
    ↓
  Worker prints the streamed text
    ↓
  forge-ledger records the completed trade
    ↓
  persisted ledger snapshot + optional settlement export

Future multi-hop execution:
  Seed keeps early layers locally
    ↓ activation tensor (encrypted)
  Remote peers execute contiguous layer ranges
    ↓ text fragments stream back to the requester
```

## Worker Node Schema

```json
{
  "node_id": "forge_2b8f...a3c1",
  "hardware": {
    "cpu": "Apple M4",
    "cores": 10,
    "memory_gb": 16,
    "metal": true,
    "unified_memory": true
  },
  "network": {
    "bandwidth_mbps": 200,
    "region": "JP",
    "nat_type": "restricted_cone"
  },
  "status": {
    "available_memory_gb": 12.5,
    "battery_pct": null,
    "power_state": "plugged_in",
    "assigned_layers": [8, 15],
    "reputation": 0.95
  }
}
```
