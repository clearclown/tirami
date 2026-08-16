use thiserror::Error;

#[derive(Error, Debug)]
pub enum TiramiError {
    #[error("model not found: {0}")]
    ModelNotFound(String),

    #[error("failed to load model: {0}")]
    ModelLoadError(String),

    #[error("inference error: {0}")]
    InferenceError(String),

    #[error("invalid layer range: {start}..{end}")]
    InvalidLayerRange { start: u32, end: u32 },

    #[error("peer not found: {0}")]
    PeerNotFound(String),

    #[error("network error: {0}")]
    NetworkError(String),

    #[error("shard assignment failed: {0}")]
    ShardAssignmentError(String),

    #[error("ledger error: {0}")]
    LedgerError(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// Phase 14.2 — no suitable provider in PeerRegistry, or other scheduling failure.
    #[error("scheduling error: {0}")]
    SchedulingError(String),

    /// Phase 14.2 — consumer does not have enough TRM to reserve for an inference.
    #[error("insufficient balance: need {need} TRM, have {have} TRM")]
    InsufficientBalance { need: u64, have: u64 },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Operator config file could not be read or parsed (#162).
    #[error("config error: {0}")]
    Config(String),
}
