use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Economy
// ---------------------------------------------------------------------------

/// Response from `GET /v1/tirami/balance`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Balance {
    pub node_id: String,
    pub contributed: u64,
    pub consumed: u64,
    pub reserved: u64,
    pub net_balance: i64,
    pub effective_balance: i64,
    pub reputation: f64,
}

/// Response from `GET /v1/tirami/pricing`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Pricing {
    pub trm_per_token: f64,
    pub supply_factor: f64,
    pub demand_factor: f64,
    /// How much purchasing power 1 CU has (grows with network adoption).
    #[serde(default)]
    pub cu_purchasing_power: f64,
    pub deflation_factor: f64,
    pub total_trades_ever: u64,
    pub estimated_cost_100_tokens: u64,
    pub estimated_cost_1000_tokens: u64,
}

// ---------------------------------------------------------------------------
// Inference
// ---------------------------------------------------------------------------

/// Response from `POST /v1/chat/completions`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletion {
    pub id: String,
    pub model: String,
    pub choices: Vec<serde_json::Value>,
    pub usage: serde_json::Value,
    /// Forge extension: CU cost and resulting balance after inference.
    #[serde(default)]
    pub x_tirami: Option<TiramiUsage>,
}

/// Forge-specific extension field in chat completion responses.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TiramiUsage {
    pub trm_cost: u64,
    pub effective_balance: i64,
}

// ---------------------------------------------------------------------------
// Phase 14.1 — PeerRegistry response
// ---------------------------------------------------------------------------

/// A single peer entry from `GET /v1/tirami/peers`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub price_multiplier: f64,
    pub available_cu: u64,
    pub models: Vec<String>,
    pub latency_hint_ms: u64,
    pub latency_ema_ms: f64,
    pub last_seen: u64,
    pub audit_tier: String,
    pub verified_trades: u64,
}

/// Response from `GET /v1/tirami/peers`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeersResponse {
    pub count: usize,
    pub peers: Vec<PeerInfo>,
}

// ---------------------------------------------------------------------------
// Phase 14.2 — Schedule probe response
// ---------------------------------------------------------------------------

/// Response from `POST /v1/tirami/schedule` — what the ledger would pick.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Schedule {
    /// Hex NodeId of the provider that `select_provider` would choose.
    pub provider: String,
    /// TRM the consumer would need to reserve.
    pub estimated_trm_cost: u64,
    pub model_id: String,
    pub max_tokens: u64,
}

// ---------------------------------------------------------------------------
// Phase 16 — Anchor history response
// ---------------------------------------------------------------------------

/// A single batch submission observed via ChainClient.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnchorSubmission {
    pub batch_id: u64,
    pub tx_hash: String,
    pub merkle_root_hex: String,
    pub submitted_at_ms: u64,
    pub node_count: usize,
    pub flops_total: u64,
}

/// Response from `GET /v1/tirami/anchors`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnchorsResponse {
    pub count: usize,
    pub anchors: Vec<AnchorSubmission>,
}
