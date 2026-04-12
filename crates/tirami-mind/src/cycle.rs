//! Single improvement cycle execution.
//!
//! A cycle is the smallest atomic unit of self-improvement:
//!
//!   baseline benchmark
//!         ↓
//!   meta-optimizer proposes change
//!         ↓
//!   candidate benchmark
//!         ↓
//!   decision: Keep / Revert / Defer

use crate::benchmark::Benchmark;
use crate::budget::TrmBudget;
use crate::harness::Harness;
use crate::meta_optimizer::MetaOptimizer;
use crate::types::{BenchmarkResult, CycleDecision, ImprovementCycle, ImprovementProposal};

/// Estimated CU return per 1.0 of score improvement. Tune to your domain.
pub const ROI_CU_PER_SCORE_UNIT: u64 = 100_000;

/// Runs one improvement cycle end-to-end.
///
/// The runner enforces budget gating, runs benchmarks, asks the optimizer
/// for a proposal, evaluates the proposal, and returns a fully-populated
/// `ImprovementCycle` regardless of outcome.
pub struct ImprovementCycleRunner {
    benchmark: Box<dyn Benchmark>,
    optimizer: Box<dyn MetaOptimizer>,
    budget: TrmBudget,
    roi_cu_per_score_unit: u64,
}

impl ImprovementCycleRunner {
    pub fn new(
        benchmark: Box<dyn Benchmark>,
        optimizer: Box<dyn MetaOptimizer>,
        budget: TrmBudget,
    ) -> Self {
        Self {
            benchmark,
            optimizer,
            budget,
            roi_cu_per_score_unit: ROI_CU_PER_SCORE_UNIT,
        }
    }

    pub fn with_roi_unit(
        benchmark: Box<dyn Benchmark>,
        optimizer: Box<dyn MetaOptimizer>,
        budget: TrmBudget,
        roi_cu_per_score_unit: u64,
    ) -> Self {
        Self {
            benchmark,
            optimizer,
            budget,
            roi_cu_per_score_unit,
        }
    }

    /// Execute one improvement cycle on the given harness.
    ///
    /// `now_ms` is injected for testability (no wall-clock dependency).
    pub async fn run_one(&mut self, harness: &Harness, now_ms: u64) -> ImprovementCycle {
        // Day rollover check.
        self.budget.maybe_reset_day(now_ms);

        // Gate: are we allowed to start a cycle today?
        if !self.budget.can_start_cycle() {
            return Self::defer_cycle(harness, "daily cycle limit reached");
        }

        let _ = self.budget.record_cycle_start();

        // 1. Baseline benchmark
        let baseline = self.benchmark.evaluate(harness);
        self.record_benchmark_cost(&baseline);

        // 2. Meta-optimizer proposes a change
        let proposal = self.optimizer.propose(harness, &baseline).await;

        // Gate: can we afford the proposal cost?
        if proposal.trm_cost_to_propose > 0 {
            if !self.budget.can_spend(proposal.trm_cost_to_propose) {
                return ImprovementCycle::new(
                    baseline.clone(),
                    proposal,
                    baseline,
                    CycleDecision::Defer,
                    0.0,
                    0.0,
                );
            }
            let _ = self.budget.record_spend(proposal.trm_cost_to_propose);
        }

        // 3. Candidate benchmark
        let candidate = self.benchmark.evaluate(&proposal.proposed_harness);
        self.record_benchmark_cost(&candidate);

        // 4. Decision
        let delta = candidate.score - baseline.score;
        let trm_invested = proposal.trm_cost_to_propose
            + baseline.cu_consumed
            + candidate.cu_consumed;
        let cu_return_estimate = (delta.max(0.0) * self.roi_cu_per_score_unit as f64) as u64;
        let roi = if trm_invested > 0 {
            cu_return_estimate as f64 / trm_invested as f64
        } else {
            f64::INFINITY
        };

        let keep = self.budget.is_improvement_worth_keeping(
            delta,
            trm_invested,
            cu_return_estimate,
        );
        let decision = if keep {
            CycleDecision::Keep
        } else {
            CycleDecision::Revert
        };

        // roi_cu: use 0.0 for infinity (matching Python)
        let roi_cu = if roi.is_infinite() { 0.0 } else { roi };

        ImprovementCycle::new(baseline, proposal, candidate, decision, delta, roi_cu)
    }

    /// Access the budget (read-only).
    pub fn budget(&self) -> &TrmBudget {
        &self.budget
    }

    /// Access the budget (mutable).
    pub fn budget_mut(&mut self) -> &mut TrmBudget {
        &mut self.budget
    }

    fn record_benchmark_cost(&mut self, result: &BenchmarkResult) {
        if result.cu_consumed > 0 && self.budget.can_spend(result.cu_consumed) {
            let _ = self.budget.record_spend(result.cu_consumed);
        }
    }

    fn defer_cycle(harness: &Harness, reason: &str) -> ImprovementCycle {
        let zero_result = BenchmarkResult::new(harness.version, 0.0, 1, 0, 0);
        let zero_proposal = ImprovementProposal {
            proposed_harness: harness.clone(),
            proposer_model: "(deferred)".to_string(),
            rationale: reason.to_string(),
            trm_cost_to_propose: 0,
        };
        ImprovementCycle::new(
            zero_result.clone(),
            zero_proposal,
            zero_result,
            CycleDecision::Defer,
            0.0,
            0.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::InMemoryBenchmark;
    use crate::budget::TrmBudget;
    use crate::harness::Harness;
    use crate::meta_optimizer::{EchoMetaOptimizer, PromptRewriteOptimizer};

    fn make_runner(
        scoring_fn: impl Fn(&Harness) -> f64 + Send + Sync + 'static,
        optimizer: impl MetaOptimizer + 'static,
        trm_cost_per_benchmark: u64,
    ) -> ImprovementCycleRunner {
        let benchmark = InMemoryBenchmark::new("test", scoring_fn, 100, trm_cost_per_benchmark);
        let budget = TrmBudget {
            min_score_delta: 0.001,
            min_roi_threshold: 0.0,
            ..TrmBudget::default()
        };
        ImprovementCycleRunner::new(
            Box::new(benchmark),
            Box::new(optimizer),
            budget,
        )
    }

    #[tokio::test]
    async fn test_no_change_proposal_results_in_revert() {
        let mut runner = make_runner(|_| 0.5, EchoMetaOptimizer, 0);
        let cycle = runner.run_one(&Harness::new("x".to_string()), 0).await;
        // Echo optimizer doesn't change anything → score is identical → REVERT
        assert_eq!(cycle.decision, CycleDecision::Revert);
        assert_eq!(cycle.delta, 0.0);
    }

    #[tokio::test]
    async fn test_improving_proposal_is_kept() {
        let mut runner = make_runner(
            |h| if h.system_prompt.contains("concise") { 0.85 } else { 0.5 },
            PromptRewriteOptimizer::with_fn(|p| format!("{} concise", p)),
            0,
        );
        let cycle = runner.run_one(&Harness::new("hello".to_string()), 0).await;
        assert_eq!(cycle.decision, CycleDecision::Keep);
        assert!((cycle.delta - (0.85 - 0.5)).abs() < 1e-10);
        assert_eq!(cycle.proposal.proposed_harness.version, 2);
    }

    #[tokio::test]
    async fn test_regressing_proposal_is_reverted() {
        let mut runner = make_runner(
            |h| if h.system_prompt.contains("bad") { 0.3 } else { 0.7 },
            PromptRewriteOptimizer::with_fn(|p| format!("{} bad", p)),
            0,
        );
        let cycle = runner.run_one(&Harness::new("hello".to_string()), 0).await;
        assert_eq!(cycle.decision, CycleDecision::Revert);
        assert!(cycle.delta < 0.0);
    }

    #[tokio::test]
    async fn test_cycle_records_budget_spend() {
        let benchmark = InMemoryBenchmark::new("test", |_| 0.5_f64, 100, 10u64);
        let budget = TrmBudget {
            min_score_delta: 0.001,
            min_roi_threshold: 0.0,
            ..TrmBudget::default()
        };
        let opt = PromptRewriteOptimizer::new(|p: &str| p.to_string(), "local-rewrite", 100u64);
        let mut runner = ImprovementCycleRunner::new(
            Box::new(benchmark),
            Box::new(opt),
            budget,
        );
        runner.run_one(&Harness::new("x".to_string()), 0).await;
        assert!(runner.budget().spent_today_cu >= 100); // at least the proposal cost
        assert_eq!(runner.budget().cycles_today, 1);
    }

    #[tokio::test]
    async fn test_proposal_too_expensive_is_deferred() {
        let mut runner = make_runner(
            |_| 0.5,
            PromptRewriteOptimizer::new(|p: &str| format!("{}x", p), "local-rewrite", 10_000_000u64),
            0,
        );
        let cycle = runner.run_one(&Harness::new("x".to_string()), 0).await;
        assert_eq!(cycle.decision, CycleDecision::Defer);
    }

    #[tokio::test]
    async fn test_daily_cycle_limit_defers() {
        let benchmark = InMemoryBenchmark::with_fn(|_| 0.5_f64);
        let budget = TrmBudget {
            max_cycles_per_day: 2,
            ..TrmBudget::default()
        };
        let mut runner = ImprovementCycleRunner::new(
            Box::new(benchmark),
            Box::new(EchoMetaOptimizer),
            budget,
        );
        runner.run_one(&Harness::new("x".to_string()), 0).await;
        runner.run_one(&Harness::new("x".to_string()), 0).await;
        let cycle3 = runner.run_one(&Harness::new("x".to_string()), 0).await;
        assert_eq!(cycle3.decision, CycleDecision::Defer);
    }

    #[tokio::test]
    async fn test_min_score_delta_gates_keep() {
        let benchmark = InMemoryBenchmark::with_fn(|h: &Harness| {
            if h.version == 1 { 0.500 } else { 0.501 }
        });
        let budget = TrmBudget {
            min_score_delta: 0.05,
            min_roi_threshold: 0.0,
            ..TrmBudget::default()
        };
        let opt = PromptRewriteOptimizer::with_fn(|p| format!("{} ", p));
        let mut runner = ImprovementCycleRunner::new(
            Box::new(benchmark),
            Box::new(opt),
            budget,
        );
        let cycle = runner.run_one(&Harness::new("x".to_string()), 0).await;
        // 0.001 improvement < 0.05 threshold → REVERT
        assert_eq!(cycle.decision, CycleDecision::Revert);
    }
}
