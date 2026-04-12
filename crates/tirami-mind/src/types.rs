//! Core data types for forge-mind.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::budget::TrmBudget;
use crate::harness::Harness;

/// Outcome of running a harness against a benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub harness_version: u64,
    /// Score in [0.0, 1.0]
    pub score: f64,
    pub sample_count: u32,
    pub duration_ms: u64,
    pub cu_consumed: u64,
    /// Per-sub-test scores, optional extra detail.
    #[serde(default)]
    pub details: HashMap<String, f64>,
}

impl BenchmarkResult {
    pub fn new(
        harness_version: u64,
        score: f64,
        sample_count: u32,
        duration_ms: u64,
        cu_consumed: u64,
    ) -> Self {
        let score = score.clamp(0.0, 1.0);
        Self {
            harness_version,
            score,
            sample_count,
            duration_ms,
            cu_consumed,
            details: HashMap::new(),
        }
    }
}

/// A meta-optimizer's suggested change to a harness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementProposal {
    pub proposed_harness: Harness,
    pub proposer_model: String,
    pub rationale: String,
    pub trm_cost_to_propose: u64,
}

/// Outcome of evaluating an improvement proposal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CycleDecision {
    /// Apply the proposal, bump version.
    Keep,
    /// Discard the proposal.
    Revert,
    /// Couldn't decide; budget exhausted or benchmark invalid.
    Defer,
}

/// A complete benchmark → propose → benchmark → decide cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementCycle {
    pub baseline: BenchmarkResult,
    pub proposal: ImprovementProposal,
    pub candidate: BenchmarkResult,
    pub decision: CycleDecision,
    pub delta: f64,
    pub roi_cu: f64,
}

impl ImprovementCycle {
    pub fn new(
        baseline: BenchmarkResult,
        proposal: ImprovementProposal,
        candidate: BenchmarkResult,
        decision: CycleDecision,
        delta: f64,
        roi_cu: f64,
    ) -> Self {
        // Auto-compute delta if it is zero but scores differ (matching Python __post_init__)
        let delta = if delta == 0.0 && candidate.score != baseline.score {
            candidate.score - baseline.score
        } else {
            delta
        };
        Self {
            baseline,
            proposal,
            candidate,
            decision,
            delta,
            roi_cu,
        }
    }
}

/// Serializable snapshot of a `TiramiMindAgent`'s persisted state.
///
/// The `optimizer` and `benchmark` fields are NOT persisted because they are
/// trait objects (`Box<dyn MetaOptimizer>` / `Box<dyn Benchmark>`). They must
/// be re-attached by the caller (e.g. by calling `/v1/tirami/mind/init` again
/// after a restart). Once re-attached, the handler merges the snapshot back
/// via `TiramiMindAgent::restore_from_snapshot`.
///
/// Phase 10 TODO: add HMAC-SHA256 integrity check if snapshot tampering becomes a concern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindAgentSnapshot {
    pub harness: Harness,
    pub history: Vec<ImprovementCycle>,
    pub budget: TrmBudget,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_result_clamps_above_one() {
        let r = BenchmarkResult::new(1, 1.5, 1, 0, 0);
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn test_benchmark_result_clamps_below_zero() {
        let r = BenchmarkResult::new(1, -0.3, 1, 0, 0);
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn test_improvement_cycle_auto_computes_delta() {
        let harness = Harness::new("test".to_string());
        let proposal = ImprovementProposal {
            proposed_harness: harness.clone(),
            proposer_model: "test".to_string(),
            rationale: "test".to_string(),
            trm_cost_to_propose: 0,
        };
        let baseline = BenchmarkResult::new(1, 0.5, 1, 0, 0);
        let candidate = BenchmarkResult::new(1, 0.7, 1, 0, 0);
        let cycle = ImprovementCycle::new(baseline, proposal, candidate, CycleDecision::Keep, 0.0, 0.0);
        // delta was 0.0 but scores differ → auto-computed
        assert!((cycle.delta - 0.2).abs() < 1e-10);
    }

    #[test]
    fn test_cycle_decision_eq() {
        assert_eq!(CycleDecision::Keep, CycleDecision::Keep);
        assert_ne!(CycleDecision::Keep, CycleDecision::Revert);
    }
}
