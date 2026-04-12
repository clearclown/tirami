//! forge-bank — Layer 2 of the Forge ecosystem.
//!
//! Financial instruments built on top of the `forge-ledger` lending primitives:
//! strategies, portfolios, futures contracts, insurance, and risk modeling.
//!
//! This is the Rust rewrite of the original Python scaffold (archived under
//! `clearclown/forge-bank`). All semantics — constants, formulas, validation
//! rules — are preserved bit-for-bit.

#![allow(dead_code)]

pub mod errors;
pub mod futures;
pub mod insurance;
pub mod portfolio;
pub mod risk;
pub mod strategies;
pub mod types;
pub mod yield_optimizer;

// Public re-exports
pub use errors::BankError;
pub use futures::{futures_pnl, mark_to_market, required_margin, FuturesContract};
pub use insurance::{premium_for, InsurancePolicy};
pub use portfolio::PortfolioManager;
pub use risk::{RiskAssessment, RiskModel};
pub use strategies::{BalancedStrategy, ConservativeStrategy, HighYieldStrategy, Strategy, StrategyKind};
pub use types::{
    ActionKind, Decision, PoolSnapshot, Portfolio, Position, PositionKind, RiskTolerance,
};
pub use yield_optimizer::YieldOptimizer;
