use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

/// Unique identifier for a node, derived from its Ed25519 public key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    pub fn from_public_key(key: &VerifyingKey) -> Self {
        Self(key.to_bytes())
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(value: &str) -> Result<Self, String> {
        // Accept both the current `tirami_` prefix and the legacy
        // `forge_` prefix (pre-rename snapshots / config files).
        let value = value
            .strip_prefix("tirami_")
            .or_else(|| value.strip_prefix("forge_"))
            .unwrap_or(value);
        let bytes = hex::decode(value).map_err(|e| format!("decode node id: {e}"))?;
        if bytes.len() != 32 {
            return Err(format!("expected 32 bytes, got {}", bytes.len()));
        }

        let mut node = [0u8; 32];
        node.copy_from_slice(&bytes);
        Ok(Self(node))
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tirami_{}", hex::encode(&self.0[..8]))
    }
}

impl std::str::FromStr for NodeId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// Identifier for a model (e.g., "llama-3.2-1b-q4").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelId(pub String);

/// A contiguous range of transformer layers assigned to a single node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerRange {
    pub start: u32,
    pub end: u32, // exclusive
}

impl LayerRange {
    pub fn new(start: u32, end: u32) -> Self {
        assert!(start < end, "LayerRange: start must be < end");
        Self { start, end }
    }

    pub fn count(&self) -> u32 {
        self.end - self.start
    }

    pub fn contains(&self, layer: u32) -> bool {
        layer >= self.start && layer < self.end
    }
}

/// Metadata about a model, parsed from GGUF file headers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub id: ModelId,
    pub total_layers: u32,
    pub hidden_dim: u32,
    pub vocab_size: u32,
    pub head_count: u32,
    pub kv_head_count: u32,
    pub context_length: u32,
    pub file_size_bytes: u64,
    pub quantization: String, // e.g., "Q4_0", "Q4_K_M"
}

impl ModelManifest {
    /// Estimate FLOP per token for this model (Phase 15 Step 3).
    ///
    /// Formula (approximate transformer forward pass):
    ///   2 × hidden_dim² × total_layers × 3
    ///
    /// Factor 2 = multiply + add per matmul element.
    /// Factor 3 = Q + K + V projections per attention layer (FFN folded in).
    ///
    /// This is the foundational metric for Tirami's core principle:
    /// **1 TRM = 10⁹ FLOP of verified useful computation.**
    /// Used by `record_api_trade` to populate `TradeRecord::flops_estimated`.
    pub fn flops_per_token(&self) -> u64 {
        2u64.saturating_mul(self.hidden_dim as u64)
            .saturating_mul(self.hidden_dim as u64)
            .saturating_mul(self.total_layers as u64)
            .saturating_mul(3)
    }
}

/// Computation meter reading (Phase 15 Step 3).
///
/// Records computational cost of an inference execution. Used to:
/// - Verify the "1 TRM = 10⁹ FLOP" principle in trade records
/// - Feed provider performance tracking (wall_time_ms → latency EMA)
/// - Support audit verdicts (hash of deterministic output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterReading {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub flops_estimated: u64,
    pub wall_time_ms: u64,
    pub model_id: ModelId,
}

/// Authorization token for a single inference execution (Phase 15 Step 4).
///
/// Created atomically by `ComputeLedger::begin_inference()` (which performs
/// provider selection + CU reservation in a single locked section).
/// Consumed by `settle_inference()` which executes the trade and releases
/// any excess reservation.
///
/// This pattern prevents races where the same TRM could be spent twice on
/// parallel inference requests.
#[derive(Debug, Clone)]
pub struct InferenceTicket {
    /// Monotonic id assigned by the ledger.
    pub request_id: u64,
    /// Consumer that requested the inference.
    pub consumer: NodeId,
    /// Provider selected by `select_provider`.
    pub provider: NodeId,
    /// Model identifier (matches the provider's price signal).
    pub model_id: ModelId,
    /// TRM reserved on the consumer's balance. Excess is released at settle.
    pub reserved_trm: u64,
    /// Maximum tokens allowed for this request.
    pub max_tokens: u64,
    /// True if the ticket triggers a Phase 14.3 audit challenge.
    pub audit_required: bool,
    /// Unix ms when the ticket was issued.
    pub created_at: u64,
}

/// A node's hardware and network capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapability {
    pub node_id: NodeId,
    pub cpu_cores: u16,
    pub memory_gb: f32,
    pub metal_available: bool,
    pub bandwidth_mbps: f32,
    pub battery_pct: Option<u8>,
    pub available_memory_gb: f32,
    pub region: String,
}

/// A single stage in the inference pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub node_id: NodeId,
    pub layer_range: LayerRange,
    pub position: u8,
}

/// Full pipeline topology for distributed inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTopology {
    pub model_id: ModelId,
    pub stages: Vec<PipelineStage>,
}

/// Data type for tensors in transit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DType {
    F16,
    F32,
    I8,
}

/// Metadata for an activation tensor being transmitted between pipeline stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorMeta {
    pub shape: Vec<u32>,
    pub dtype: DType,
    pub byte_len: u32,
}

/// A unit of compute work performed by a node (for the ledger).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkUnit {
    pub node_id: NodeId,
    pub timestamp: u64,
    pub layers_computed: LayerRange,
    pub model_id: ModelId,
    pub tokens_processed: u64,
    pub estimated_flops: u64,
}

/// A node's compute balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeBalance {
    pub node_id: NodeId,
    pub contributed: u64,
    pub consumed: u64,
    #[serde(default)]
    pub reserved: u64,
    pub reputation: f64,
}

impl NodeBalance {
    pub fn balance(&self) -> i64 {
        self.contributed as i64 - self.consumed as i64
    }

    pub fn available_balance(&self) -> i64 {
        self.balance() - self.reserved as i64
    }
}

// ---------------------------------------------------------------------------
// Phase 14.1 — PriceSignal (gossip-distributed per-node market quote)
// ---------------------------------------------------------------------------

/// A node's advertised price and capacity, broadcast via gossip.
///
/// Each provider emits a PriceSignal periodically (default 30s) stating
/// its current price multiplier, available capacity, and served models.
/// Consumers read these from their local PeerRegistry to select providers.
///
/// `price_multiplier` is a float relative to base tier pricing:
///   0.5 = half price (offering discount to attract load)
///   1.0 = standard price
///   2.0 = premium (node is busy, raising price to shed load)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceSignal {
    /// Provider node announcing the price.
    pub node_id: NodeId,
    /// Multiplier applied to base tier price. Must be finite and > 0.
    pub price_multiplier: f64,
    /// Available compute capacity in TRM-equivalent units.
    pub available_cu: u64,
    /// Model IDs this node can currently serve.
    pub model_capabilities: Vec<ModelId>,
    /// Self-reported latency hint in milliseconds (p50).
    pub latency_hint_ms: u32,
    /// Unix timestamp (ms) when this signal was created.
    pub timestamp: u64,
    /// Phase 19 / Tier-C enabler (fix for #80 scope-extension).
    ///
    /// Optional HTTP endpoint the provider advertises for callers
    /// that want to drive inference over the OpenAI-compatible REST
    /// surface rather than iroh P2P. When present, consumers can
    /// resolve `NodeId → URL` locally from the peer registry
    /// instead of requiring the user to hand-wire `peer.url` on
    /// every request.
    ///
    /// `None` is the pre-Phase-19 wire shape and parses cleanly
    /// (via `#[serde(default)]`) — operators who don't want to
    /// advertise HTTP simply leave the config field empty.
    ///
    /// SECURITY: the advertised URL is self-attested. Consumers
    /// MUST still verify trades are dual-signed (which already
    /// happens). An attacker advertising an HTTP endpoint they
    /// don't control at most wastes the consumer's request; they
    /// cannot forge a signed trade.
    #[serde(default)]
    pub http_endpoint: Option<String>,
}

impl PriceSignal {
    /// Minimum valid multiplier (prevents zero or negative).
    pub const MIN_MULTIPLIER: f64 = 0.01;
    /// Maximum valid multiplier (prevents absurd prices).
    pub const MAX_MULTIPLIER: f64 = 100.0;

    pub fn is_valid(&self) -> bool {
        self.price_multiplier.is_finite()
            && self.price_multiplier >= Self::MIN_MULTIPLIER
            && self.price_multiplier <= Self::MAX_MULTIPLIER
    }
}

// ---------------------------------------------------------------------------
// Phase 14.3 — AuditTier (implementation lives in tirami-ledger, type here)
// ---------------------------------------------------------------------------

/// Audit frequency tier — determines how often a node gets verified.
///
/// Nodes progress Unverified → Probationary → Established → Trusted → Staked
/// as they accumulate verified trades and reputation. Failed audits cause
/// regression. The `audit_probability()` return value is the probability
/// that a single inference from this provider will be audited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditTier {
    /// New node, no verified history. Every request audited.
    Unverified,
    /// < 10 verified trades. 50% of requests audited.
    Probationary,
    /// 10-100 trades + reputation > 0.6. 10% audited.
    Established,
    /// 100+ trades + reputation > 0.8. 1% audited.
    Trusted,
    /// Active stake + Trusted reputation. 0.1% audited.
    Staked,
}

impl AuditTier {
    /// Probability that this tier triggers an audit on a single trade (0.0 - 1.0).
    pub fn audit_probability(self) -> f64 {
        match self {
            AuditTier::Unverified => 1.0,
            AuditTier::Probationary => 0.5,
            AuditTier::Established => 0.1,
            AuditTier::Trusted => 0.01,
            AuditTier::Staked => 0.001,
        }
    }

    /// Promote to the next tier. Returns self if already at top.
    pub fn promote(self) -> Self {
        match self {
            AuditTier::Unverified => AuditTier::Probationary,
            AuditTier::Probationary => AuditTier::Established,
            AuditTier::Established => AuditTier::Trusted,
            AuditTier::Trusted => AuditTier::Staked,
            AuditTier::Staked => AuditTier::Staked,
        }
    }

    /// Demote to the previous tier. Returns self if already Unverified.
    pub fn demote(self) -> Self {
        match self {
            AuditTier::Unverified => AuditTier::Unverified,
            AuditTier::Probationary => AuditTier::Unverified,
            AuditTier::Established => AuditTier::Probationary,
            AuditTier::Trusted => AuditTier::Established,
            AuditTier::Staked => AuditTier::Trusted,
        }
    }
}

impl Default for AuditTier {
    fn default() -> Self {
        AuditTier::Unverified
    }
}

#[cfg(test)]
mod tests {
    use super::NodeId;

    #[test]
    fn node_id_hex_roundtrip() {
        let original = NodeId([7u8; 32]);
        let parsed = original.to_hex().parse::<NodeId>().unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn node_id_display_uses_tirami_prefix() {
        let id = NodeId([0xABu8; 32]);
        let shown = format!("{}", id);
        assert!(
            shown.starts_with("tirami_"),
            "expected tirami_ prefix, got {shown}"
        );
    }

    #[test]
    fn node_id_parser_accepts_tirami_prefix() {
        let raw = format!("tirami_{}", hex::encode([0x12u8; 32]));
        let parsed = NodeId::from_hex(&raw).unwrap();
        assert_eq!(parsed, NodeId([0x12u8; 32]));
    }

    #[test]
    fn node_id_parser_accepts_legacy_forge_prefix() {
        // Backward compat (fix #77): saved snapshots / config files
        // from the pre-rename era must still parse.
        let raw = format!("forge_{}", hex::encode([0x34u8; 32]));
        let parsed = NodeId::from_hex(&raw).unwrap();
        assert_eq!(parsed, NodeId([0x34u8; 32]));
    }

    #[test]
    fn node_id_parser_accepts_bare_hex() {
        let raw = hex::encode([0x56u8; 32]);
        let parsed = NodeId::from_hex(&raw).unwrap();
        assert_eq!(parsed, NodeId([0x56u8; 32]));
    }

    // ------------------------------------------------------------------
    // Phase 14.1 tests — PriceSignal validation
    // ------------------------------------------------------------------

    #[test]
    fn price_signal_rejects_nan_multiplier() {
        let sig = super::PriceSignal {
            node_id: NodeId([0u8; 32]),
            price_multiplier: f64::NAN,
            available_cu: 100,
            model_capabilities: vec![],
            latency_hint_ms: 50,
            timestamp: 0,
            http_endpoint: None,
        };
        assert!(!sig.is_valid());
    }

    #[test]
    fn price_signal_rejects_zero_multiplier() {
        let sig = super::PriceSignal {
            node_id: NodeId([0u8; 32]),
            price_multiplier: 0.0,
            available_cu: 100,
            model_capabilities: vec![],
            latency_hint_ms: 50,
            timestamp: 0,
            http_endpoint: None,
        };
        assert!(!sig.is_valid());
    }

    #[test]
    fn price_signal_rejects_infinite_multiplier() {
        let sig = super::PriceSignal {
            node_id: NodeId([0u8; 32]),
            price_multiplier: f64::INFINITY,
            available_cu: 100,
            model_capabilities: vec![],
            latency_hint_ms: 50,
            timestamp: 0,
            http_endpoint: None,
        };
        assert!(!sig.is_valid());
    }

    #[test]
    fn price_signal_rejects_negative_multiplier() {
        let sig = super::PriceSignal {
            node_id: NodeId([0u8; 32]),
            price_multiplier: -1.0,
            available_cu: 100,
            model_capabilities: vec![],
            latency_hint_ms: 50,
            timestamp: 0,
            http_endpoint: None,
        };
        assert!(!sig.is_valid());
    }

    #[test]
    fn price_signal_rejects_absurd_multiplier() {
        let sig = super::PriceSignal {
            node_id: NodeId([0u8; 32]),
            price_multiplier: 1000.0,
            available_cu: 100,
            model_capabilities: vec![],
            latency_hint_ms: 50,
            timestamp: 0,
            http_endpoint: None,
        };
        assert!(!sig.is_valid());
    }

    #[test]
    fn price_signal_accepts_normal_multiplier() {
        let sig = super::PriceSignal {
            node_id: NodeId([0u8; 32]),
            price_multiplier: 1.0,
            available_cu: 100,
            model_capabilities: vec![],
            latency_hint_ms: 50,
            timestamp: 0,
            http_endpoint: None,
        };
        assert!(sig.is_valid());
    }

    #[test]
    fn price_signal_accepts_discount() {
        let sig = super::PriceSignal {
            node_id: NodeId([0u8; 32]),
            price_multiplier: 0.5,
            available_cu: 100,
            model_capabilities: vec![],
            latency_hint_ms: 50,
            timestamp: 0,
            http_endpoint: None,
        };
        assert!(sig.is_valid());
    }

    // ------------------------------------------------------------------
    // Phase 14.1 tests — AuditTier progression
    // ------------------------------------------------------------------

    #[test]
    fn audit_tier_default_is_unverified() {
        assert_eq!(super::AuditTier::default(), super::AuditTier::Unverified);
    }

    #[test]
    fn audit_tier_probabilities_decrease_monotonically() {
        let tiers = [
            super::AuditTier::Unverified,
            super::AuditTier::Probationary,
            super::AuditTier::Established,
            super::AuditTier::Trusted,
            super::AuditTier::Staked,
        ];
        for pair in tiers.windows(2) {
            assert!(pair[0].audit_probability() > pair[1].audit_probability());
        }
    }

    #[test]
    fn audit_tier_unverified_audits_always() {
        assert_eq!(super::AuditTier::Unverified.audit_probability(), 1.0);
    }

    #[test]
    fn audit_tier_staked_audits_rarely() {
        assert!(super::AuditTier::Staked.audit_probability() < 0.01);
    }

    #[test]
    fn audit_tier_promote_chain() {
        let mut tier = super::AuditTier::Unverified;
        tier = tier.promote();
        assert_eq!(tier, super::AuditTier::Probationary);
        tier = tier.promote();
        assert_eq!(tier, super::AuditTier::Established);
        tier = tier.promote();
        assert_eq!(tier, super::AuditTier::Trusted);
        tier = tier.promote();
        assert_eq!(tier, super::AuditTier::Staked);
        // Top of chain — no further promotion.
        assert_eq!(tier.promote(), super::AuditTier::Staked);
    }

    #[test]
    fn audit_tier_demote_chain() {
        let mut tier = super::AuditTier::Staked;
        tier = tier.demote();
        assert_eq!(tier, super::AuditTier::Trusted);
        tier = tier.demote();
        assert_eq!(tier, super::AuditTier::Established);
        tier = tier.demote();
        assert_eq!(tier, super::AuditTier::Probationary);
        tier = tier.demote();
        assert_eq!(tier, super::AuditTier::Unverified);
        // Bottom — no further demotion.
        assert_eq!(tier.demote(), super::AuditTier::Unverified);
    }
}
