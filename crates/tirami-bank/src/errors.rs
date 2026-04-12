//! Error types for forge-bank.

/// Top-level error type for forge-bank operations.
#[derive(Debug, thiserror::Error)]
pub enum BankError {
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("insufficient cash: need {needed}, have {have}")]
    InsufficientCash { needed: u64, have: u64 },
}
