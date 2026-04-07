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
pub mod cycle;
pub mod agent;

pub use errors::MindError;
pub use types::{BenchmarkResult, CycleDecision, ImprovementCycle, ImprovementProposal};
pub use harness::Harness;
pub use budget::CuBudget;
pub use benchmark::{Benchmark, InMemoryBenchmark};
pub use meta_optimizer::{EchoMetaOptimizer, MetaOptimizer, PromptRewriteOptimizer};
pub use cycle::ImprovementCycleRunner;
pub use agent::ForgeMindAgent;
