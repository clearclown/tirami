//! forge-mind — Layer 3 of the Forge ecosystem.
//!
//! AutoAgent-style self-improvement loops where the meta-agent is paid for
//! in CU rather than driven by a human.
//!
//! This is the Rust rewrite of the original Python scaffold (archived under
//! `clearclown/forge-mind`). All semantics are preserved bit-for-bit.

pub mod errors;
pub mod types;
pub mod harness;
pub mod budget;
pub mod benchmark;
pub mod meta_optimizer;
pub mod cu_paid_optimizer;
pub mod cycle;
pub mod agent;
pub mod federated;

pub use errors::MindError;
pub use types::{BenchmarkResult, CycleDecision, ImprovementCycle, ImprovementProposal, MindAgentSnapshot};
pub use harness::Harness;
pub use budget::TrmBudget;
pub use benchmark::{Benchmark, InMemoryBenchmark};
pub use meta_optimizer::{EchoMetaOptimizer, MetaOptimizer, PromptRewriteOptimizer};
pub use cu_paid_optimizer::TrmPaidOptimizer;
pub use cycle::ImprovementCycleRunner;
pub use agent::{TiramiMindAgent, Stats as MindStats};
pub use federated::{
    Aggregator, AggregationResult, FederatedError, FederatedRound,
    GradientContribution, WeightedAverageAggregator,
};
