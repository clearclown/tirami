//! TiramiMindAgent — high-level autonomous self-improvement loop facade.
//!
//! Wraps a Harness, a Benchmark, a MetaOptimizer, a TrmBudget, and an
//! ImprovementCycleRunner into a single object that runs N cycles and tracks
//! the harness evolution.

use crate::benchmark::Benchmark;
use crate::budget::TrmBudget;
use crate::cycle::ImprovementCycleRunner;
use crate::harness::Harness;
use crate::meta_optimizer::MetaOptimizer;
use crate::types::{CycleDecision, ImprovementCycle, MindAgentSnapshot};

/// A self-improving agent driven by CU-budgeted cycles.
pub struct TiramiMindAgent {
    pub harness: Harness,
    runner: ImprovementCycleRunner,
    history: Vec<ImprovementCycle>,
}

impl TiramiMindAgent {
    pub fn new(
        harness: Harness,
        benchmark: Box<dyn Benchmark>,
        optimizer: Box<dyn MetaOptimizer>,
        budget: Option<TrmBudget>,
    ) -> Self {
        let budget = budget.unwrap_or_default();
        let runner = ImprovementCycleRunner::new(benchmark, optimizer, budget);
        Self {
            harness,
            runner,
            history: Vec::new(),
        }
    }

    /// Run up to `n_cycles` improvement cycles.
    ///
    /// Stops early if the budget is exhausted or N cycles complete.
    /// Returns the list of cycles actually executed.
    ///
    /// `now_ms` is injected for testability (no wall-clock dependency).
    pub async fn improve(&mut self, n_cycles: usize, now_ms: u64) -> Vec<ImprovementCycle> {
        let mut executed = Vec::new();
        for _ in 0..n_cycles {
            let cycle = self.runner.run_one(&self.harness, now_ms).await;
            let decision = cycle.decision.clone();

            if decision == CycleDecision::Keep {
                self.harness = cycle.proposal.proposed_harness.clone();
            }

            self.history.push(cycle.clone());
            executed.push(cycle);

            if decision == CycleDecision::Defer {
                // Budget hit a hard limit — stop the loop early.
                break;
            }
            // Revert: keep current harness, try again next cycle
        }
        executed
    }

    pub fn cycle_count(&self) -> usize {
        self.history.len()
    }

    pub fn kept_count(&self) -> usize {
        self.history
            .iter()
            .filter(|c| c.decision == CycleDecision::Keep)
            .count()
    }

    pub fn reverted_count(&self) -> usize {
        self.history
            .iter()
            .filter(|c| c.decision == CycleDecision::Revert)
            .count()
    }

    pub fn deferred_count(&self) -> usize {
        self.history
            .iter()
            .filter(|c| c.decision == CycleDecision::Defer)
            .count()
    }

    pub fn total_trm_invested(&self) -> u64 {
        self.history
            .iter()
            .map(|c| {
                c.proposal.trm_cost_to_propose
                    + c.baseline.cu_consumed
                    + c.candidate.cu_consumed
            })
            .sum()
    }

    pub fn stats(&self) -> Stats {
        let latest_score = self.history.last().map(|c| c.candidate.score).unwrap_or(0.0);
        let first_score = self.history.first().map(|c| c.baseline.score).unwrap_or(0.0);
        Stats {
            harness_version: self.harness.version,
            cycle_count: self.cycle_count(),
            kept: self.kept_count(),
            reverted: self.reverted_count(),
            deferred: self.deferred_count(),
            total_trm_invested: self.total_trm_invested(),
            first_score,
            latest_score,
            score_delta: latest_score - first_score,
        }
    }

    pub fn history(&self) -> &[ImprovementCycle] {
        &self.history
    }

    /// Access the cycle runner's budget (read-only).
    pub fn runner_budget(&self) -> &TrmBudget {
        self.runner.budget()
    }

    /// Access the cycle runner's budget (mutable), e.g. to update limits.
    pub fn runner_budget_mut(&mut self) -> &mut TrmBudget {
        self.runner.budget_mut()
    }

    /// Produce a serializable snapshot of persistent state.
    ///
    /// The optimizer and benchmark are NOT included — they must be re-attached
    /// after load via `restore_from_snapshot`.
    pub fn snapshot(&self) -> MindAgentSnapshot {
        MindAgentSnapshot {
            harness: self.harness.clone(),
            history: self.history.clone(),
            budget: self.runner.budget().clone(),
        }
    }

    /// Restore harness, history, and budget from a previously persisted snapshot.
    ///
    /// The optimizer and benchmark (held by the runner) are NOT replaced; they
    /// continue using whatever was provided at construction time.
    pub fn restore_from_snapshot(&mut self, snap: MindAgentSnapshot) {
        self.harness = snap.harness;
        self.history = snap.history;
        *self.runner.budget_mut() = snap.budget;
    }
}

/// Summary statistics for a TiramiMindAgent run.
#[derive(Debug, Clone)]
pub struct Stats {
    pub harness_version: u64,
    pub cycle_count: usize,
    pub kept: usize,
    pub reverted: usize,
    pub deferred: usize,
    pub total_trm_invested: u64,
    pub first_score: f64,
    pub latest_score: f64,
    pub score_delta: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::InMemoryBenchmark;
    use crate::budget::TrmBudget;
    use crate::harness::Harness;
    use crate::meta_optimizer::{EchoMetaOptimizer, PromptRewriteOptimizer};

    fn lenient_budget() -> TrmBudget {
        TrmBudget {
            min_score_delta: 0.001,
            min_roi_threshold: 0.0,
            ..TrmBudget::default()
        }
    }

    #[tokio::test]
    async fn test_agent_keeps_improving_proposal() {
        let h = Harness::new("hello".to_string());
        let bench = InMemoryBenchmark::with_fn(|h: &Harness| {
            if h.system_prompt.contains("concise") { 0.85 } else { 0.5 }
        });
        let opt = PromptRewriteOptimizer::with_fn(|p| format!("{} concise", p));
        let mut agent = TiramiMindAgent::new(
            h,
            Box::new(bench),
            Box::new(opt),
            Some(lenient_budget()),
        );
        agent.improve(1, 0).await;
        assert_eq!(agent.harness.system_prompt, "hello concise");
        assert_eq!(agent.harness.version, 2);
        assert_eq!(agent.kept_count(), 1);
        assert_eq!(agent.reverted_count(), 0);
    }

    #[tokio::test]
    async fn test_agent_reverts_regressing_proposal() {
        let h = Harness::new("hello".to_string());
        let bench = InMemoryBenchmark::with_fn(|h: &Harness| {
            if h.system_prompt.contains("concise") { 0.3 } else { 0.7 }
        });
        let opt = PromptRewriteOptimizer::with_fn(|p| format!("{} concise", p));
        let mut agent = TiramiMindAgent::new(
            h,
            Box::new(bench),
            Box::new(opt),
            Some(lenient_budget()),
        );
        agent.improve(1, 0).await;
        // Harness unchanged (still v1)
        assert_eq!(agent.harness.system_prompt, "hello");
        assert_eq!(agent.harness.version, 1);
        assert_eq!(agent.kept_count(), 0);
        assert_eq!(agent.reverted_count(), 1);
    }

    #[tokio::test]
    async fn test_agent_runs_multiple_cycles() {
        let h = Harness::new("hi".to_string());
        let bench = InMemoryBenchmark::with_fn(|h: &Harness| {
            (0.5 + 0.1 * h.system_prompt.matches("good").count() as f64).min(1.0)
        });
        let opt = PromptRewriteOptimizer::with_fn(|p| format!("{} good", p));
        let mut agent = TiramiMindAgent::new(
            h,
            Box::new(bench),
            Box::new(opt),
            Some(lenient_budget()),
        );
        agent.improve(5, 0).await;
        assert_eq!(agent.harness.system_prompt.matches("good").count(), 5);
        assert_eq!(agent.harness.version, 6); // started at 1, +5
        assert_eq!(agent.kept_count(), 5);

        let stats = agent.stats();
        assert!(stats.score_delta >= 0.45 - 0.001);
    }

    #[tokio::test]
    async fn test_agent_stops_at_budget_exhaustion() {
        let h = Harness::new("x".to_string());
        let bench = InMemoryBenchmark::with_fn(|_| 0.5_f64);
        let opt = PromptRewriteOptimizer::with_fn(|p| format!("{}.", p));
        let budget = TrmBudget {
            max_cycles_per_day: 2,
            ..TrmBudget::default()
        };
        let mut agent = TiramiMindAgent::new(
            h,
            Box::new(bench),
            Box::new(opt),
            Some(budget),
        );
        let cycles = agent.improve(10, 0).await;
        // First 2 are real (REVERT due to no score change), third is DEFER → loop stops
        assert_eq!(cycles.len(), 3);
        assert_eq!(cycles.last().unwrap().decision, CycleDecision::Defer);
    }

    #[test]
    fn test_agent_stats_initial_state() {
        let agent = TiramiMindAgent::new(
            Harness::new("x".to_string()),
            Box::new(InMemoryBenchmark::with_fn(|_| 0.5_f64)),
            Box::new(PromptRewriteOptimizer::with_fn(|p| p.to_string())),
            None,
        );
        let stats = agent.stats();
        assert_eq!(stats.cycle_count, 0);
        assert_eq!(stats.kept, 0);
        assert_eq!(stats.harness_version, 1);
    }

    #[tokio::test]
    async fn test_agent_total_trm_invested() {
        let h = Harness::new("x".to_string());
        let bench = InMemoryBenchmark::new("test", |_| 0.5_f64, 100, 10u64);
        let opt = PromptRewriteOptimizer::new(|p: &str| p.to_string(), "local-rewrite", 50u64);
        let mut agent = TiramiMindAgent::new(h, Box::new(bench), Box::new(opt), Some(lenient_budget()));
        agent.improve(1, 0).await;
        // proposal=50, baseline=10, candidate=10 → 70
        assert_eq!(agent.total_trm_invested(), 70);
    }

    #[tokio::test]
    async fn test_agent_echo_optimizer_reverts() {
        let h = Harness::new("test".to_string());
        let bench = InMemoryBenchmark::with_fn(|_| 0.5_f64);
        let opt = EchoMetaOptimizer;
        let mut agent = TiramiMindAgent::new(h, Box::new(bench), Box::new(opt), Some(lenient_budget()));
        agent.improve(3, 0).await;
        // EchoMetaOptimizer: score never changes → all REVERT
        assert_eq!(agent.kept_count(), 0);
        assert_eq!(agent.reverted_count(), 3);
        assert_eq!(agent.harness.version, 1); // unchanged
    }
}
