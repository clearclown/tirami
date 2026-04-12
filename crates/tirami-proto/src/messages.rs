use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use tirami_core::{LayerRange, ModelId, NodeId, PeerCapability, PipelineStage, TensorMeta};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    /// Lender proposes a loan to a borrower, pre-signed.
    LoanProposal(LoanProposal),
    /// Borrower accepts the loan with counter-signature.
    LoanAccept(LoanAccept),
    /// Gossip: broadcast a dual-signed loan to the mesh.
    LoanGossip(LoanGossip),
    /// Gossip: broadcast a reputation observation to the mesh.
    ReputationGossip(ReputationObservation),
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
    pub trm_amount: u64,
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
    pub trm_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
    pub provider_sig: Vec<u8>,
    pub consumer_sig: Vec<u8>,
}

// --- Loan Signing (CU Lending — Phase 5.5) ---

/// Loan proposal from lender to borrower.
///
/// Lender pre-signs the canonical bytes; borrower verifies terms and
/// counter-signs via `LoanAccept`. Mirrors `TradeProposal`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanProposal {
    /// Correlates proposal and accept messages.
    pub request_id: u64,
    pub lender: NodeId,
    pub borrower: NodeId,
    pub principal_trm: u64,
    pub interest_rate_per_hour: f64,
    pub term_hours: u64,
    pub collateral_trm: u64,
    pub created_at: u64,
    pub due_at: u64,
    /// Ed25519 signature by lender over canonical loan bytes.
    pub lender_sig: Vec<u8>,
}

/// Borrower's acceptance and counter-signature for a `LoanProposal`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanAccept {
    pub request_id: u64,
    /// Ed25519 signature by borrower over the same canonical loan bytes.
    pub borrower_sig: Vec<u8>,
}

/// Fully dual-signed loan record broadcast to the mesh.
///
/// This is the wire representation of `SignedLoanRecord` used by the gossip
/// protocol (see `forge-net::gossip::broadcast_loan`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanGossip {
    pub lender: NodeId,
    pub borrower: NodeId,
    pub principal_trm: u64,
    pub interest_rate_per_hour: f64,
    pub term_hours: u64,
    pub collateral_trm: u64,
    pub created_at: u64,
    pub due_at: u64,
    pub lender_sig: Vec<u8>,
    pub borrower_sig: Vec<u8>,
}

// --- Reputation Gossip (Phase 9 A3) ---

/// Reputation observation gossip: node X announces its observation of node Y's
/// reputation, derived from its local view of trade / repayment / uptime history.
///
/// Receiving nodes merge these into a weighted-median consensus so no single
/// observer can dominate the score (A5 collusion resistance depends on this).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReputationObservation {
    /// Node making the observation (the broadcaster).
    pub observer: NodeId,
    /// Node being observed.
    pub subject: NodeId,
    /// Observed reputation in [0.0, 1.0], computed by the observer from its
    /// local view of the subject's trade / repayment / uptime history.
    pub reputation: f64,
    /// Number of trades the observer saw involving the subject.
    /// Used as a weight when merging observations from multiple observers.
    pub trade_count: u64,
    /// Total CU volume involved in observed trades.
    pub total_trm_volume: u64,
    /// Observer's timestamp (ms since epoch).
    pub timestamp_ms: u64,
    /// Ed25519 signature by the observer over canonical_bytes().
    /// Must be exactly 64 bytes (Phase 10 strict mode — empty sig is rejected).
    pub signature: Vec<u8>,
}

impl ReputationObservation {
    /// Create a new signed observation.
    ///
    /// The caller provides the observer's Ed25519 signing key; the observation
    /// fields are hashed canonically and signed.  The resulting 64-byte
    /// signature is stored in `signature`.
    pub fn new_signed(
        subject: NodeId,
        reputation: f64,
        trade_count: u64,
        total_trm_volume: u64,
        timestamp_ms: u64,
        signing_key: &SigningKey,
    ) -> Self {
        let observer = NodeId(signing_key.verifying_key().to_bytes());
        let mut obs = Self {
            observer,
            subject,
            reputation,
            trade_count,
            total_trm_volume,
            timestamp_ms,
            signature: Vec::new(),
        };
        let canonical = obs.canonical_bytes();
        let sig: Signature = signing_key.sign(&canonical);
        obs.signature = sig.to_bytes().to_vec();
        obs
    }

    /// Deterministic bytes used for signing and deduplication.
    /// Format: observer(32) + subject(32) + reputation(8,f64 BE) +
    ///         trade_count(8) + total_trm_volume(8) + timestamp_ms(8).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 32 + 8 + 8 + 8 + 8);
        buf.extend_from_slice(&self.observer.0);
        buf.extend_from_slice(&self.subject.0);
        buf.extend_from_slice(&self.reputation.to_be_bytes());
        buf.extend_from_slice(&self.trade_count.to_be_bytes());
        buf.extend_from_slice(&self.total_trm_volume.to_be_bytes());
        buf.extend_from_slice(&self.timestamp_ms.to_be_bytes());
        buf
    }

    /// Verify the Ed25519 signature over canonical_bytes() using the observer's
    /// public key (= observer.0).
    ///
    /// # Phase 10 strict mode
    /// Empty or wrong-length signatures are rejected.  Only a valid 64-byte
    /// Ed25519 signature from the declared observer key passes.
    pub fn verify(&self) -> bool {
        // Empty sig is rejected in strict Phase 10 mode.
        if self.signature.len() != 64 {
            return false;
        }
        let sig_bytes: [u8; 64] = match self.signature.as_slice().try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&sig_bytes);
        let Ok(vk) = VerifyingKey::from_bytes(&self.observer.0) else {
            return false;
        };
        vk.verify(&self.canonical_bytes(), &sig).is_ok()
    }

    /// SHA-256 of canonical_bytes() — used as gossip dedup key.
    pub fn dedup_key(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"rep:");
        hasher.update(&self.canonical_bytes());
        hasher.finalize().into()
    }
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
    #[error("invalid loan field {field}: {reason}")]
    InvalidLoanField { field: String, reason: String },
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
            Payload::LoanProposal(p) => {
                // Lender must be the sender (they initiated).
                if p.lender != *sender {
                    return Err(ProtocolValidationError::SenderMismatch {
                        expected: sender.to_hex(),
                        actual: p.lender.to_hex(),
                    });
                }
                if p.principal_trm == 0 {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "principal_trm".into(),
                        reason: "must be non-zero".into(),
                    });
                }
                if p.term_hours == 0 {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "term_hours".into(),
                        reason: "must be non-zero".into(),
                    });
                }
                if p.lender_sig.len() != 64 {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "lender_sig".into(),
                        reason: "must be 64 bytes (Ed25519)".into(),
                    });
                }
                Ok(())
            }
            Payload::LoanAccept(a) => {
                if a.borrower_sig.len() != 64 {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "borrower_sig".into(),
                        reason: "must be 64 bytes (Ed25519)".into(),
                    });
                }
                Ok(())
            }
            Payload::LoanGossip(g) => {
                if g.lender_sig.len() != 64 {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "lender_sig".into(),
                        reason: "must be 64 bytes (Ed25519)".into(),
                    });
                }
                if g.borrower_sig.len() != 64 {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "borrower_sig".into(),
                        reason: "must be 64 bytes (Ed25519)".into(),
                    });
                }
                if g.principal_trm == 0 {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "principal_trm".into(),
                        reason: "must be non-zero".into(),
                    });
                }
                Ok(())
            }
            Payload::ReputationGossip(obs) => {
                // Observer must match the envelope sender.
                if obs.observer != *sender {
                    return Err(ProtocolValidationError::SenderMismatch {
                        expected: sender.to_hex(),
                        actual: obs.observer.to_hex(),
                    });
                }
                // Reputation must be in [0, 1].
                if !obs.reputation.is_finite() || !(0.0..=1.0).contains(&obs.reputation) {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "reputation".into(),
                        reason: "must be finite and within [0.0, 1.0]".into(),
                    });
                }
                // Signature must be exactly 64 bytes (Phase 10 strict mode).
                if obs.signature.len() != 64 {
                    return Err(ProtocolValidationError::InvalidLoanField {
                        field: "signature".into(),
                        reason: "must be 64 bytes (Ed25519)".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn loan_proposal_round_trips_via_bincode() {
        let msg = LoanProposal {
            request_id: 42,
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 1_000,
            interest_rate_per_hour: 0.001,
            term_hours: 72,
            collateral_trm: 3_000,
            created_at: 1_700_000_000_000,
            due_at: 1_700_000_000_000 + 72 * 3_600_000,
            lender_sig: vec![0u8; 64],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let back: LoanProposal = bincode::deserialize(&bytes).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn loan_accept_round_trips() {
        let msg = LoanAccept {
            request_id: 42,
            borrower_sig: vec![1u8; 64],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let back: LoanAccept = bincode::deserialize(&bytes).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn loan_gossip_round_trips() {
        let msg = LoanGossip {
            lender: NodeId([5u8; 32]),
            borrower: NodeId([6u8; 32]),
            principal_trm: 5_000,
            interest_rate_per_hour: 0.002,
            term_hours: 48,
            collateral_trm: 15_000,
            created_at: 1,
            due_at: 1 + 48 * 3_600_000,
            lender_sig: vec![2u8; 64],
            borrower_sig: vec![3u8; 64],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let back: LoanGossip = bincode::deserialize(&bytes).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn reputation_observation_round_trips_via_bincode() {
        let obs = ReputationObservation {
            observer: NodeId([1u8; 32]),
            subject: NodeId([2u8; 32]),
            reputation: 0.75,
            trade_count: 42,
            total_trm_volume: 4_200,
            timestamp_ms: 1_700_000_000_000,
            signature: vec![],
        };
        let bytes = bincode::serialize(&obs).unwrap();
        let back: ReputationObservation = bincode::deserialize(&bytes).unwrap();
        assert_eq!(obs, back);
    }

    #[test]
    fn reputation_observation_dedup_key_is_deterministic() {
        let obs = ReputationObservation {
            observer: NodeId([1u8; 32]),
            subject: NodeId([2u8; 32]),
            reputation: 0.5,
            trade_count: 10,
            total_trm_volume: 1_000,
            timestamp_ms: 1_000,
            signature: vec![],
        };
        assert_eq!(obs.dedup_key(), obs.dedup_key());
        let obs2 = ReputationObservation { reputation: 0.6, ..obs.clone() };
        assert_ne!(obs.dedup_key(), obs2.dedup_key());
    }

    #[test]
    fn reputation_observation_verify_empty_sig_fails_strict() {
        // Phase 10: empty signature is no longer accepted.
        let obs = ReputationObservation {
            observer: NodeId([1u8; 32]),
            subject: NodeId([2u8; 32]),
            reputation: 0.5,
            trade_count: 10,
            total_trm_volume: 500,
            timestamp_ms: 1_000,
            signature: vec![],
        };
        assert!(!obs.verify(), "empty signature must be rejected in Phase 10 strict mode");
    }

    #[test]
    fn reputation_observation_verify_wrong_length_sig_fails() {
        let obs = ReputationObservation {
            observer: NodeId([1u8; 32]),
            subject: NodeId([2u8; 32]),
            reputation: 0.5,
            trade_count: 10,
            total_trm_volume: 500,
            timestamp_ms: 1_000,
            signature: vec![0u8; 32], // wrong length
        };
        assert!(!obs.verify(), "wrong-length signature should fail");
    }

    // === Phase 10 P2: Ed25519 signing tests ===

    fn make_observation_signed(key: &SigningKey) -> ReputationObservation {
        ReputationObservation::new_signed(
            NodeId([2u8; 32]),
            0.8,
            10,
            1_000,
            1_700_000_000_000,
            key,
        )
    }

    #[test]
    fn test_new_signed_produces_64_byte_signature() {
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let obs = make_observation_signed(&key);
        assert_eq!(obs.signature.len(), 64, "signature must be 64 bytes");
    }

    #[test]
    fn test_signed_observation_verifies_successfully() {
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let obs = make_observation_signed(&key);
        assert!(obs.verify(), "fresh signed observation must verify");
    }

    #[test]
    fn test_observation_verification_fails_for_wrong_pubkey() {
        // Sign with one key, tamper observer to a different key — must fail.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_observation_signed(&key);
        obs.observer = NodeId([0xab; 32]); // different public key
        assert!(!obs.verify(), "tampered observer key must fail verification");
    }

    #[test]
    fn test_observation_verification_fails_for_tampered_reputation() {
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_observation_signed(&key);
        obs.reputation = 1.0; // tamper field after signing
        assert!(!obs.verify(), "tampered reputation field must fail verification");
    }

    #[test]
    fn loan_proposal_validation_rejects_zero_principal() {
        let p = LoanProposal {
            request_id: 1,
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 0,
            interest_rate_per_hour: 0.001,
            term_hours: 24,
            collateral_trm: 100,
            created_at: 0,
            due_at: 24 * 3_600_000,
            lender_sig: vec![0u8; 64],
        };
        let payload = Payload::LoanProposal(p);
        let err = payload.validate_with_sender(&NodeId([1u8; 32]));
        assert!(err.is_err());
    }

    // =========================================================================
    // Security tests: Ed25519 signature attacks on ReputationObservation
    // =========================================================================

    fn make_signed_obs_with_key(key: &SigningKey) -> ReputationObservation {
        ReputationObservation::new_signed(
            NodeId([0xBB; 32]),
            0.8,
            20,
            2_000,
            1_700_000_000_000,
            key,
        )
    }

    #[test]
    fn test_reputation_obs_rejects_signature_with_flipped_bit() {
        // A single flipped bit in the signature must cause verify() to return false.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_signed_obs_with_key(&key);
        // Flip the very first bit of byte 0
        obs.signature[0] ^= 0x01;
        assert!(!obs.verify(), "flipped-bit signature must be rejected");
    }

    #[test]
    fn test_reputation_obs_rejects_all_zero_signature() {
        // A 64-byte all-zero "signature" is structurally valid length-wise
        // but cryptographically invalid.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_signed_obs_with_key(&key);
        obs.signature = vec![0u8; 64];
        assert!(!obs.verify(), "all-zero 64-byte signature must be rejected");
    }

    #[test]
    fn test_reputation_obs_rejects_all_ff_signature() {
        // A 64-byte all-0xFF signature is not a valid Ed25519 signature.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_signed_obs_with_key(&key);
        obs.signature = vec![0xFFu8; 64];
        assert!(!obs.verify(), "all-0xFF 64-byte signature must be rejected");
    }

    #[test]
    fn test_reputation_obs_rejects_replay_with_different_subject() {
        // Sign for subject A, swap in subject B — the same signature must not verify.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let obs_a = make_signed_obs_with_key(&key);
        // Clone the whole struct and swap only the subject.
        let mut obs_b = obs_a.clone();
        obs_b.subject = NodeId([0xCC; 32]); // different subject
        assert!(obs_a.verify(), "original must verify");
        assert!(!obs_b.verify(), "swapped-subject replay must be rejected");
    }

    #[test]
    fn test_reputation_obs_rejects_replay_with_different_reputation() {
        // Valid sig over rep=0.8; bump to rep=0.99 — must fail.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_signed_obs_with_key(&key);
        let original_sig = obs.signature.clone();
        obs.reputation = 0.99;
        obs.signature = original_sig;
        assert!(!obs.verify(), "tampered-reputation replay must be rejected");
    }

    #[test]
    fn test_reputation_obs_rejects_replay_with_different_timestamp() {
        // Valid sig over timestamp=T; swap timestamp to T+1000 — must fail.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_signed_obs_with_key(&key);
        let original_sig = obs.signature.clone();
        obs.timestamp_ms += 1000;
        obs.signature = original_sig;
        assert!(!obs.verify(), "tampered-timestamp replay must be rejected");
    }

    #[test]
    fn test_reputation_obs_rejects_signature_signed_by_different_key() {
        // Sign with key A; the `observer` field still points at key A's pubkey,
        // but the signature is produced by key B → verify() must fail.
        let key_a = SigningKey::generate(&mut rand::rngs::OsRng);
        let key_b = SigningKey::generate(&mut rand::rngs::OsRng);
        let obs_a = make_signed_obs_with_key(&key_a);
        let obs_b = make_signed_obs_with_key(&key_b);
        // Put key_b's signature into key_a's observation (observer pubkey = key_a).
        let mut tampered = obs_a.clone();
        tampered.signature = obs_b.signature.clone();
        assert!(obs_a.verify(), "original must verify");
        assert!(!tampered.verify(), "cross-key signature must be rejected");
    }

    #[test]
    fn test_reputation_obs_rejects_signature_truncated_to_32_bytes() {
        // A 32-byte prefix of a valid 64-byte signature must fail.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let obs = make_signed_obs_with_key(&key);
        let mut truncated = obs.clone();
        truncated.signature = obs.signature[..32].to_vec();
        assert!(!truncated.verify(), "32-byte truncated signature must be rejected");
    }

    #[test]
    fn test_reputation_gossip_validation_rejects_out_of_range_reputation() {
        // reputation = 1.5 is out of [0.0, 1.0] — validate_with_sender must error.
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_signed_obs_with_key(&key);
        obs.reputation = 1.5;
        let payload = Payload::ReputationGossip(obs.clone());
        let err = payload.validate_with_sender(&obs.observer);
        assert!(err.is_err(), "out-of-range reputation must be rejected by protocol validation");
    }

    #[test]
    fn test_reputation_gossip_validation_rejects_negative_reputation() {
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut obs = make_signed_obs_with_key(&key);
        obs.reputation = -0.1;
        let payload = Payload::ReputationGossip(obs.clone());
        let err = payload.validate_with_sender(&obs.observer);
        assert!(err.is_err(), "negative reputation must be rejected by protocol validation");
    }

    // =========================================================================
    // Security tests: LoanProposal / LoanGossip signature-length enforcement
    // =========================================================================

    #[test]
    fn test_loan_gossip_rejects_short_lender_sig() {
        let msg = Payload::LoanGossip(LoanGossip {
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 1_000,
            interest_rate_per_hour: 0.001,
            term_hours: 24,
            collateral_trm: 500,
            created_at: 1_000,
            due_at: 1_000 + 24 * 3_600_000,
            lender_sig: vec![0u8; 32], // too short — must be 64
            borrower_sig: vec![0u8; 64],
        });
        assert!(msg.validate_with_sender(&NodeId([0u8; 32])).is_err(),
            "short lender_sig must be rejected");
    }

    #[test]
    fn test_loan_gossip_rejects_short_borrower_sig() {
        let msg = Payload::LoanGossip(LoanGossip {
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 1_000,
            interest_rate_per_hour: 0.001,
            term_hours: 24,
            collateral_trm: 500,
            created_at: 1_000,
            due_at: 1_000 + 24 * 3_600_000,
            lender_sig: vec![0u8; 64],
            borrower_sig: vec![0u8; 16], // too short
        });
        assert!(msg.validate_with_sender(&NodeId([0u8; 32])).is_err(),
            "short borrower_sig must be rejected");
    }

    #[test]
    fn test_loan_gossip_rejects_zero_principal() {
        let msg = Payload::LoanGossip(LoanGossip {
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 0, // zero — invalid
            interest_rate_per_hour: 0.001,
            term_hours: 24,
            collateral_trm: 500,
            created_at: 1_000,
            due_at: 1_000 + 24 * 3_600_000,
            lender_sig: vec![0u8; 64],
            borrower_sig: vec![0u8; 64],
        });
        assert!(msg.validate_with_sender(&NodeId([0u8; 32])).is_err(),
            "zero principal in LoanGossip must be rejected");
    }

    #[test]
    fn test_loan_proposal_rejects_wrong_sender() {
        // Lender field says node [1u8;32] but sender is [2u8;32] → mismatch.
        let p = LoanProposal {
            request_id: 1,
            lender: NodeId([1u8; 32]),
            borrower: NodeId([3u8; 32]),
            principal_trm: 1_000,
            interest_rate_per_hour: 0.001,
            term_hours: 24,
            collateral_trm: 500,
            created_at: 0,
            due_at: 24 * 3_600_000,
            lender_sig: vec![0u8; 64],
        };
        let err = Payload::LoanProposal(p).validate_with_sender(&NodeId([2u8; 32]));
        assert!(err.is_err(), "lender/sender mismatch must be rejected");
    }
}
