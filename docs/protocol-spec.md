# Forge — Wire Protocol Specification

## Overview

Forge nodes exchange bincode-serialized control messages over encrypted QUIC connections established by Iroh. Activation tensors are carried as raw bytes inside `Forward` messages. The current v1 implementation uses the same envelope for local seed/requester inference and for future multi-hop pipeline messages.

## Message Envelope

Every message is wrapped in an envelope:

```rust
pub struct Envelope {
    pub msg_id: u64,
    pub sender: NodeId,
    pub timestamp: u64, // unix millis
    pub payload: Payload,
}
```

## Payload Enum

```rust
pub enum Payload {
    Hello(Hello),
    Welcome(Welcome),
    AssignShard(AssignShard),
    ShardReady(ShardReady),
    PipelineTopology(PipelineTopologyMsg),
    Forward(Forward),
    TokenResult(TokenResult),
    InferenceRequest(InferenceRequest),
    TokenStream(TokenStreamMsg),
    Heartbeat(Heartbeat),
    Ping(Ping),
    Pong(Pong),
    Leaving(Leaving),
    Rebalance(Rebalance),
}
```

## Discovery and Handshake

```rust
pub struct Hello {
    pub version: u16,
    pub capability: PeerCapability,
}

pub struct Welcome {
    pub version: u16,
    pub capability: PeerCapability,
    pub known_peers: Vec<PeerInfo>,
}

pub struct PeerInfo {
    pub node_id: NodeId,
    pub addr: String,
}
```

- `version` is the protocol version advertised by the sender.
- `capability` describes CPU, memory, bandwidth, and region for scheduling decisions.
- `known_peers` is an opportunistic peer list, not a globally authoritative registry.

## Shard Assignment

These messages define the future multi-hop layer pipeline. They are part of v1 even though the current reference implementation mainly runs whole-model inference on the seed.

```rust
pub struct AssignShard {
    pub model_id: ModelId,
    pub model_source: String,
    pub layer_range: LayerRange,
    pub pipeline_position: u8,
    pub upstream: Option<NodeId>,
    pub downstream: Option<NodeId>,
}

pub struct ShardReady {
    pub model_id: ModelId,
    pub layer_range: LayerRange,
    pub load_time_ms: u64,
}

pub struct PipelineTopologyMsg {
    pub model_id: ModelId,
    pub stages: Vec<PipelineStage>,
}
```

## Inference Messages

### Forward

`Forward` carries an activation tensor between pipeline stages.

```rust
pub struct Forward {
    pub request_id: u64,
    pub sequence_pos: u32,
    pub tensor_meta: TensorMeta,
    #[serde(with = "serde_bytes")]
    pub tensor_data: Vec<u8>,
}

pub struct TensorMeta {
    pub shape: Vec<u32>,
    pub dtype: DType,
    pub byte_len: u32,
}
```

- `tensor_data` is raw activation bytes.
- `dtype` is one of `F16`, `F32`, or `I8`.
- WAN transport is expected to prefer compact representations such as `I8`.

### TokenResult

`TokenResult` is reserved for final-stage sampled token IDs in multi-hop inference.

```rust
pub struct TokenResult {
    pub request_id: u64,
    pub tokens: Vec<u32>,
}
```

### InferenceRequest

The current seed/requester flow sends prompt text directly. The seed tokenizes locally.

```rust
pub struct InferenceRequest {
    pub request_id: u64,
    pub prompt_text: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
}
```

- `prompt_text` replaces the earlier token-ID prompt hack.
- `max_tokens` is both a generation limit and the basis for preflight CU affordability checks.

### TokenStreamMsg

The current streaming response sends decoded text fragments rather than token IDs.

```rust
pub struct TokenStreamMsg {
    pub request_id: u64,
    pub text: String,
    pub is_final: bool,
}
```

- `text` is a decoded text fragment suitable for immediate rendering.
- `is_final = true` closes the stream for the request.
- Current insufficient-balance behavior is encoded as a final text fragment such as `[error: insufficient CU balance]`. A typed error payload is planned for a later protocol revision.

## Health and Liveness

```rust
pub struct Heartbeat {
    pub uptime_sec: u64,
    pub load: f32,
    pub memory_free_gb: f32,
    pub battery_pct: Option<u8>,
}

pub struct Ping {
    pub sent_at: u64,
}

pub struct Pong {
    pub ping_sent_at: u64,
    pub received_at: u64,
}
```

## Cluster Management

```rust
pub enum LeaveReason {
    Shutdown,
    LowBattery,
    UserRequest,
}

pub struct Leaving {
    pub reason: LeaveReason,
    pub drain_time_ms: u64,
}

pub enum RebalanceReason {
    NodeJoined,
    NodeLeft,
    ModelUpgrade,
}

pub struct Rebalance {
    pub new_topology: PipelineTopologyMsg,
    pub reason: RebalanceReason,
}
```

## Serialization Rules

- Control messages use bincode.
- `Forward.tensor_data` is transmitted as raw contiguous bytes.
- The envelope stays uniform across all message types so transports can stay generic.
- The protocol does not embed fiat, blockchain, or exchange settlement fields. Those belong in off-protocol integrations.

## Connection Lifecycle

### Current seed/requester flow

```text
Requester                       Seed
  |                              |
  |--- QUIC + encryption ------->|
  |--- Hello ------------------->|
  |<-- Welcome ------------------|
  |--- InferenceRequest -------->|
  |<-- TokenStreamMsg ---------- |
  |<-- TokenStreamMsg ---------- |
  |<-- TokenStreamMsg (final) -- |
```

### Future multi-hop flow

```text
Coordinator       Worker A        Worker B        Final Stage
    |                |               |                |
    |-- AssignShard->|               |                |
    |-- AssignShard----------------->|                |
    |-- AssignShard---------------------------------->|
    |<-- ShardReady--|               |                |
    |<---------------- ShardReady ---|                |
    |<-------------------------------- ShardReady ---|
    |-- PipelineTopology broadcast to all ----------->|
    |-- Forward ---->|-- Forward ---->|-- TokenResult->|
```

## Versioning

Current version: `1`

- Peers advertise their version through `Hello` and `Welcome`.
- The reference implementation currently assumes compatible peers and ignores unknown future payloads.
- Breaking wire changes should increment `version` and define downgrade behavior explicitly.
