//! forge-agora — Layer 4 of the Forge ecosystem.
//!
//! Post-marketing agent marketplace: reputation aggregation, capability
//! matching, and NIP-90 / A2A discovery. Built on forge-ledger agora primitives.
//!
//! See `forge-economics/spec/parameters.md` §12 for the canonical constants.

pub mod errors;
pub mod matching;
pub mod marketplace;
pub mod registry;
pub mod reputation;
pub mod types;

pub use errors::AgoraError;
pub use matching::CapabilityMatcher;
pub use marketplace::Marketplace;
pub use registry::{AgentRegistry, RegistrySnapshot};
pub use reputation::ReputationCalculator;
pub use types::{AgentProfile, CapabilityMatch, CapabilityQuery, ReputationScore, TradeObservation};

// Re-export ModelTier so callers don't need to depend on forge-ledger directly.
pub use tirami_ledger::lending::ModelTier;
