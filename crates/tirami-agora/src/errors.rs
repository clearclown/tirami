/// Errors that can be returned by forge-agora operations.
#[derive(Debug, thiserror::Error)]
pub enum AgoraError {
    #[error("invalid hex id: {0}")]
    InvalidHex(String),
    #[error("invalid trade: {0}")]
    InvalidTrade(String),
    #[error("invalid query: {0}")]
    InvalidQuery(String),
}
