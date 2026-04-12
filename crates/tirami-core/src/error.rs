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

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
