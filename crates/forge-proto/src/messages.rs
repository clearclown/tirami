use forge_core::{
    LayerRange, ModelId, NodeId, PeerCapability, PipelineStage, TensorMeta,
};
use serde::{Deserialize, Serialize};

/// Top-level message envelope for all wire protocol communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub msg_id: u64,
    pub sender: NodeId,
    pub timestamp: u64,
    pub payload: Payload,
}

/// All possible message types in the Forge wire protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    StartRpcServer(StartRpcServer),
    RpcServerReady(RpcServerReady),
    RpcServerFailed(RpcServerFailed),
}

// --- Discovery & Handshake ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub version: u16,
    pub capability: PeerCapability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Welcome {
    pub version: u16,
    pub capability: PeerCapability,
    pub known_peers: Vec<PeerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: NodeId,
    pub addr: String,
}

// --- Shard Assignment ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignShard {
    pub model_id: ModelId,
    pub model_source: String,
    pub layer_range: LayerRange,
    pub pipeline_position: u8,
    pub upstream: Option<NodeId>,
    pub downstream: Option<NodeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardReady {
    pub model_id: ModelId,
    pub layer_range: LayerRange,
    pub load_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTopologyMsg {
    pub model_id: ModelId,
    pub stages: Vec<PipelineStage>,
}

// --- Inference ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forward {
    pub request_id: u64,
    pub sequence_pos: u32,
    pub tensor_meta: TensorMeta,
    #[serde(with = "serde_bytes")]
    pub tensor_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResult {
    pub request_id: u64,
    pub tokens: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: u64,
    /// The prompt as plain text (Seed tokenizes it).
    pub prompt_text: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStreamMsg {
    pub request_id: u64,
    /// Decoded text fragment for this token.
    pub text: String,
    pub is_final: bool,
}

// --- Health ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub uptime_sec: u64,
    pub load: f32,
    pub memory_free_gb: f32,
    pub battery_pct: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ping {
    pub sent_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pong {
    pub ping_sent_at: u64,
    pub received_at: u64,
}

// --- Cluster Management ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LeaveReason {
    Shutdown,
    LowBattery,
    UserRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leaving {
    pub reason: LeaveReason,
    pub drain_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RebalanceReason {
    NodeJoined,
    NodeLeft,
    ModelUpgrade,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rebalance {
    pub new_topology: PipelineTopologyMsg,
    pub reason: RebalanceReason,
}

// --- RPC Distributed Inference ---

/// Seed tells a peer to start an rpc-server subprocess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRpcServer {
    pub model_id: ModelId,
    pub layer_range: LayerRange,
    pub port: u16,
}

/// Peer confirms rpc-server is running and ready.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcServerReady {
    pub port: u16,
}

/// Peer reports rpc-server failed to start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcServerFailed {
    pub reason: String,
}

mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        Ok(bytes)
    }
}
