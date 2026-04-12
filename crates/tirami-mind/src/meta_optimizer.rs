//! Meta-optimizer abstractions.
//!
//! A `MetaOptimizer` proposes a new harness given a current harness and
//! benchmark feedback. Implementations vary:
//!
//! - `EchoMetaOptimizer`: returns the input harness unchanged. Useful for tests.
//! - `PromptRewriteOptimizer`: applies a caller-supplied transformation function
//!   to the system prompt. Useful for testing the cycle without paying CU.
//! - `TrmPaidOptimizer` (v0.2): asks a frontier model to rewrite the harness,
//!   paying for the inference in CU via forge-sdk.

use async_trait::async_trait;

use crate::harness::Harness;
use crate::types::{BenchmarkResult, ImprovementProposal};

/// Abstract interface for harness improvement proposers.
#[async_trait]
pub trait MetaOptimizer: Send + Sync {
    fn name(&self) -> &str;
    async fn propose(&self, current: &Harness, benchmark: &BenchmarkResult) -> ImprovementProposal;
    fn estimated_trm_cost(&self) -> u64;
}

/// Returns the input harness unchanged (as a new version). Default and useful for tests.
pub struct EchoMetaOptimizer;

#[async_trait]
impl MetaOptimizer for EchoMetaOptimizer {
    fn name(&self) -> &str {
        "EchoMetaOptimizer"
    }

    async fn propose(&self, current: &Harness, _benchmark: &BenchmarkResult) -> ImprovementProposal {
        // The proposed harness must be a NEW version (not an alias to current)
        // so version semantics work. We evolve with no actual changes.
        let proposed = current.evolve(
            None,
            Some(format!("echo of v{}", current.version)),
        );
        ImprovementProposal {
            proposed_harness: proposed,
            proposer_model: "echo".to_string(),
            rationale: "no-op proposal".to_string(),
            trm_cost_to_propose: 0,
        }
    }

    fn estimated_trm_cost(&self) -> u64 {
        0
    }
}

/// Applies a caller-supplied transform to the system prompt.
///
/// Useful for testing the cycle: pass a function that, e.g., appends
/// "Be concise." to the prompt, and verify the cycle keeps it if it
/// improves the benchmark score.
pub struct PromptRewriteOptimizer {
    transform: Box<dyn Fn(&str) -> String + Send + Sync>,
    proposer_model: String,
    trm_cost_per_proposal: u64,
}

impl PromptRewriteOptimizer {
    pub fn new(
        transform: impl Fn(&str) -> String + Send + Sync + 'static,
        proposer_model: impl Into<String>,
        trm_cost_per_proposal: u64,
    ) -> Self {
        Self {
            transform: Box::new(transform),
            proposer_model: proposer_model.into(),
            trm_cost_per_proposal,
        }
    }

    /// Convenience constructor with default model name and zero cost.
    pub fn with_fn(transform: impl Fn(&str) -> String + Send + Sync + 'static) -> Self {
        Self::new(transform, "local-rewrite", 0)
    }
}

#[async_trait]
impl MetaOptimizer for PromptRewriteOptimizer {
    fn name(&self) -> &str {
        "PromptRewriteOptimizer"
    }

    async fn propose(&self, current: &Harness, _benchmark: &BenchmarkResult) -> ImprovementProposal {
        let new_prompt = (self.transform)(&current.system_prompt);
        let proposed = current.evolve(
            Some(new_prompt),
            Some(format!("prompt rewrite via {}", self.proposer_model)),
        );
        ImprovementProposal {
            proposed_harness: proposed,
            proposer_model: self.proposer_model.clone(),
            rationale: format!("applied transform on prompt at v{}", current.version),
            trm_cost_to_propose: self.trm_cost_per_proposal,
        }
    }

    fn estimated_trm_cost(&self) -> u64 {
        self.trm_cost_per_proposal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Harness;
    use crate::types::BenchmarkResult;

    fn dummy_result(harness: &Harness) -> BenchmarkResult {
        BenchmarkResult::new(harness.version, 0.5, 1, 0, 0)
    }

    #[tokio::test]
    async fn test_echo_optimizer_produces_new_version() {
        let h = Harness::new("test prompt".to_string());
        let result = dummy_result(&h);
        let opt = EchoMetaOptimizer;
        let proposal = opt.propose(&h, &result).await;
        // Echo should produce version 2 with same content
        assert_eq!(proposal.proposed_harness.version, 2);
        assert_eq!(proposal.proposed_harness.system_prompt, "test prompt");
        assert_eq!(proposal.proposer_model, "echo");
        assert_eq!(proposal.trm_cost_to_propose, 0);
    }

    #[tokio::test]
    async fn test_echo_optimizer_preserves_parent_version() {
        let h = Harness::new("hello".to_string());
        let result = dummy_result(&h);
        let opt = EchoMetaOptimizer;
        let proposal = opt.propose(&h, &result).await;
        assert_eq!(proposal.proposed_harness.parent_version, Some(1));
    }

    #[tokio::test]
    async fn test_prompt_rewrite_optimizer_transforms_prompt() {
        let h = Harness::new("hello".to_string());
        let result = dummy_result(&h);
        let opt = PromptRewriteOptimizer::with_fn(|p| format!("{} concise", p));
        let proposal = opt.propose(&h, &result).await;
        assert_eq!(proposal.proposed_harness.system_prompt, "hello concise");
        assert_eq!(proposal.proposed_harness.version, 2);
    }

    #[tokio::test]
    async fn test_prompt_rewrite_records_trm_cost() {
        let h = Harness::new("hello".to_string());
        let result = dummy_result(&h);
        let opt = PromptRewriteOptimizer::new(|p: &str| p.to_string(), "local-rewrite", 100);
        let proposal = opt.propose(&h, &result).await;
        assert_eq!(proposal.trm_cost_to_propose, 100);
    }

    #[test]
    fn test_echo_optimizer_trm_cost_is_zero() {
        let opt = EchoMetaOptimizer;
        assert_eq!(opt.estimated_trm_cost(), 0);
    }
}
