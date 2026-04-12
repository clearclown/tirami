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
