//! On-chain anchoring layer for Tirami (Phase 16).
//!
//! Provides periodic Merkle-root commitment of off-chain TRM trades to an
//! external chain (Base L2 as of Phase 16). Off-chain execution stays fast;
//! periodic on-chain writes create a tamper-evident audit trail and enable
//! external TRM purchase/withdrawal via a bridge contract.
//!
//! # Module layout
//!
//! - [`client`] — `ChainClient` trait + `MockChainClient` for testing
//! - [`anchorer`] — periodic anchoring task
//! - [`proof`] — Merkle proof encoding helpers

pub mod anchorer;
pub mod client;
pub mod proof;

pub use anchorer::{Anchorer, AnchorerConfig, AnchoringError};
pub use client::{BatchSubmission, ChainClient, ChainError, MockChainClient, TxHash};
pub use proof::{BatchDeltas, NodeDelta};
