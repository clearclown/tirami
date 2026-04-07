/// Errors for the forge-mind self-improvement layer.
#[derive(Debug, thiserror::Error)]
pub enum MindError {
    #[error("budget exhausted: {reason}")]
    BudgetExhausted { reason: String },

    #[error("invalid harness version: {0}")]
    InvalidVersion(u64),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
