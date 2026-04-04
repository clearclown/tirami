use forge_core::{LayerRange, ModelId, NodeId, PeerCapability, PipelineStage, TensorMeta};
use serde::{Deserialize, Serialize};

pub const MAX_PROTOCOL_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_PROTOCOL_PROMPT_CHARS: usize = 32 * 1024;
pub const MAX_PROTOCOL_TOKENS: u32 = 4_096;
pub const MAX_PROTOCOL_TEXT_FRAGMENT_CHARS: usize = 8 * 1024;
pub const MAX_PROTOCOL_REASON_CHARS: usize = 1_024;
pub const MAX_PROTOCOL_ERROR_MESSAGE_CHARS: usize = 1_024;

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
    Error(ErrorMsg),
    Heartbeat(Heartbeat),
    Ping(Ping),
    Pong(Pong),
    Leaving(Leaving),
    Rebalance(Rebalance),
    StartRpcServer(StartRpcServer),
    RpcServerReady(RpcServerReady),
    RpcServerFailed(RpcServerFailed),
    /// Provider proposes a trade after inference, with provider's signature.
    TradeProposal(TradeProposal),
    /// Consumer accepts the trade with counter-signature.
    TradeAccept(TradeAccept),
    /// Gossip: broadcast a dual-signed trade to the mesh.
    TradeGossip(TradeGossip),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidRequest,
    InsufficientBalance,
    Busy,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMsg {
    pub request_id: u64,
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
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

// --- Trade Signing (Proof of Useful Work) ---

/// Provider proposes a trade after completing inference.
/// Contains the trade details and provider's Ed25519 signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeProposal {
    pub request_id: u64,
    pub provider: NodeId,
    pub consumer: NodeId,
    pub cu_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
    /// Ed25519 signature from the provider over canonical trade bytes.
    pub provider_sig: Vec<u8>,
}

/// Consumer accepts the trade by counter-signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeAccept {
    pub request_id: u64,
    /// Ed25519 signature from the consumer over the same canonical trade bytes.
    pub consumer_sig: Vec<u8>,
}

/// A dual-signed trade broadcast via gossip to the mesh.
/// Any node can verify both signatures and record the trade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeGossip {
    pub provider: NodeId,
    pub consumer: NodeId,
    pub cu_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
    pub provider_sig: Vec<u8>,
    pub consumer_sig: Vec<u8>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolValidationError {
    #[error("sender mismatch: expected {expected}, got {actual}")]
    SenderMismatch { expected: String, actual: String },
    #[error("hello capability node id does not match sender")]
    HelloCapabilityMismatch,
    #[error("welcome capability node id does not match sender")]
    WelcomeCapabilityMismatch,
    #[error("invalid protocol version 0")]
    InvalidVersion,
    #[error("invalid layer range {start}..{end}")]
    InvalidLayerRange { start: u32, end: u32 },
    #[error("prompt must not be empty")]
    EmptyPrompt,
    #[error("prompt too large: {chars} chars > {limit}")]
    PromptTooLarge { chars: usize, limit: usize },
    #[error("max_tokens must be within 1..={0}")]
    InvalidMaxTokens(u32),
    #[error("temperature must be finite and within 0.0..=2.0")]
    InvalidTemperature,
    #[error("top_p must be finite and within (0.0, 1.0]")]
    InvalidTopP,
    #[error("tensor shape must not be empty")]
    EmptyTensorShape,
    #[error("tensor byte_len mismatch: meta={meta} actual={actual}")]
    TensorByteLenMismatch { meta: u32, actual: usize },
    #[error("tensor payload too large: {actual} bytes > {limit}")]
    TensorTooLarge { actual: usize, limit: usize },
    #[error("text fragment too large: {chars} chars > {limit}")]
    TextFragmentTooLarge { chars: usize, limit: usize },
    #[error("error message too large: {chars} chars > {limit}")]
    ErrorMessageTooLarge { chars: usize, limit: usize },
    #[error("rpc failure reason too large: {chars} chars > {limit}")]
    ReasonTooLarge { chars: usize, limit: usize },
    #[error("rpc port must be unprivileged and non-zero")]
    InvalidRpcPort,
}

impl Envelope {
    pub fn validate_for_peer(
        &self,
        expected_sender: &NodeId,
    ) -> Result<(), ProtocolValidationError> {
        if &self.sender != expected_sender {
            return Err(ProtocolValidationError::SenderMismatch {
                expected: expected_sender.to_hex(),
                actual: self.sender.to_hex(),
            });
        }
        self.payload.validate_with_sender(&self.sender)
    }
}

impl Payload {
    pub fn validate_with_sender(&self, sender: &NodeId) -> Result<(), ProtocolValidationError> {
        match self {
            Payload::Hello(hello) => {
                validate_protocol_version(hello.version)?;
                if hello.capability.node_id != *sender {
                    return Err(ProtocolValidationError::HelloCapabilityMismatch);
                }
                Ok(())
            }
            Payload::Welcome(welcome) => {
                validate_protocol_version(welcome.version)?;
                if welcome.capability.node_id != *sender {
                    return Err(ProtocolValidationError::WelcomeCapabilityMismatch);
                }
                Ok(())
            }
            Payload::AssignShard(assign) => {
                validate_layer_range(assign.layer_range)?;
                Ok(())
            }
            Payload::ShardReady(ready) => {
                validate_layer_range(ready.layer_range)?;
                Ok(())
            }
            Payload::PipelineTopology(topology) => {
                for stage in &topology.stages {
                    validate_layer_range(stage.layer_range)?;
                }
                Ok(())
            }
            Payload::Forward(forward) => {
                if forward.tensor_meta.shape.is_empty() {
                    return Err(ProtocolValidationError::EmptyTensorShape);
                }
                let actual = forward.tensor_data.len();
                if forward.tensor_meta.byte_len as usize != actual {
                    return Err(ProtocolValidationError::TensorByteLenMismatch {
                        meta: forward.tensor_meta.byte_len,
                        actual,
                    });
                }
                if actual > MAX_PROTOCOL_MESSAGE_BYTES {
                    return Err(ProtocolValidationError::TensorTooLarge {
                        actual,
                        limit: MAX_PROTOCOL_MESSAGE_BYTES,
                    });
                }
                Ok(())
            }
            Payload::TokenResult(_) => Ok(()),
            Payload::InferenceRequest(req) => {
                let prompt_chars = req.prompt_text.chars().count();
                if prompt_chars == 0 {
                    return Err(ProtocolValidationError::EmptyPrompt);
                }
                if prompt_chars > MAX_PROTOCOL_PROMPT_CHARS {
                    return Err(ProtocolValidationError::PromptTooLarge {
                        chars: prompt_chars,
                        limit: MAX_PROTOCOL_PROMPT_CHARS,
                    });
                }
                if req.max_tokens == 0 || req.max_tokens > MAX_PROTOCOL_TOKENS {
                    return Err(ProtocolValidationError::InvalidMaxTokens(
                        MAX_PROTOCOL_TOKENS,
                    ));
                }
                if !req.temperature.is_finite() || !(0.0..=2.0).contains(&req.temperature) {
                    return Err(ProtocolValidationError::InvalidTemperature);
                }
                if !req.top_p.is_finite() || !(0.0..=1.0).contains(&req.top_p) || req.top_p == 0.0 {
                    return Err(ProtocolValidationError::InvalidTopP);
                }
                Ok(())
            }
            Payload::TokenStream(stream) => {
                let chars = stream.text.chars().count();
                if chars > MAX_PROTOCOL_TEXT_FRAGMENT_CHARS {
                    return Err(ProtocolValidationError::TextFragmentTooLarge {
                        chars,
                        limit: MAX_PROTOCOL_TEXT_FRAGMENT_CHARS,
                    });
                }
                Ok(())
            }
            Payload::Error(error) => {
                let chars = error.message.chars().count();
                if chars > MAX_PROTOCOL_ERROR_MESSAGE_CHARS {
                    return Err(ProtocolValidationError::ErrorMessageTooLarge {
                        chars,
                        limit: MAX_PROTOCOL_ERROR_MESSAGE_CHARS,
                    });
                }
                Ok(())
            }
            Payload::Heartbeat(_) | Payload::Ping(_) | Payload::Pong(_) | Payload::Leaving(_) => {
                Ok(())
            }
            Payload::Rebalance(rebalance) => {
                for stage in &rebalance.new_topology.stages {
                    validate_layer_range(stage.layer_range)?;
                }
                Ok(())
            }
            Payload::StartRpcServer(start) => {
                validate_layer_range(start.layer_range)?;
                if start.port == 0 || start.port < 1024 {
                    return Err(ProtocolValidationError::InvalidRpcPort);
                }
                Ok(())
            }
            Payload::RpcServerReady(ready) => {
                if ready.port == 0 || ready.port < 1024 {
                    return Err(ProtocolValidationError::InvalidRpcPort);
                }
                Ok(())
            }
            Payload::RpcServerFailed(failed) => {
                let chars = failed.reason.chars().count();
                if chars > MAX_PROTOCOL_REASON_CHARS {
                    return Err(ProtocolValidationError::ReasonTooLarge {
                        chars,
                        limit: MAX_PROTOCOL_REASON_CHARS,
                    });
                }
                Ok(())
            }
            Payload::TradeProposal(proposal) => {
                if proposal.provider != *sender {
                    return Err(ProtocolValidationError::SenderMismatch {
                        expected: sender.to_hex(),
                        actual: proposal.provider.to_hex(),
                    });
                }
                if proposal.model_id.len() > 256 {
                    return Err(ProtocolValidationError::ReasonTooLarge {
                        chars: proposal.model_id.len(),
                        limit: 256,
                    });
                }
                Ok(())
            }
            Payload::TradeAccept(_) => Ok(()),
            Payload::TradeGossip(gossip) => {
                if gossip.model_id.len() > 256 {
                    return Err(ProtocolValidationError::ReasonTooLarge {
                        chars: gossip.model_id.len(),
                        limit: 256,
                    });
                }
                Ok(())
            }
        }
    }
}

/// Supported protocol versions.
const SUPPORTED_VERSIONS: &[u16] = &[1];

fn validate_protocol_version(version: u16) -> Result<(), ProtocolValidationError> {
    if !SUPPORTED_VERSIONS.contains(&version) {
        return Err(ProtocolValidationError::InvalidVersion);
    }
    Ok(())
}

fn validate_layer_range(range: LayerRange) -> Result<(), ProtocolValidationError> {
    if range.start >= range.end {
        return Err(ProtocolValidationError::InvalidLayerRange {
            start: range.start,
            end: range.end,
        });
    }
    Ok(())
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
