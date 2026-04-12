//! High-level YieldOptimizer combining strategy, portfolio, and risk.
//!
//! A `YieldOptimizer` wraps a strategy and a risk model. On each tick, it
//! runs the strategy, then validates the resulting portfolio against the
//! risk budget. If the new state would violate the risk budget, the
//! decisions are NOT applied.
//!
//! Ported from the Python scaffold `tirami_bank/yield_optimizer.py`.

use std::collections::HashMap;

use crate::portfolio::PortfolioManager;
use crate::risk::{RiskAssessment, RiskModel};
use crate::strategies::Strategy;
use crate::types::{Decision, PoolSnapshot, Portfolio, RiskTolerance};

/// Strategy + risk-budget gate.
pub struct YieldOptimizer {
    pub manager: PortfolioManager,
    pub risk_model: RiskModel,
    pub max_var_fraction: f64,
    pub rejected_count: u64,
    pub applied_count: u64,
}

/// Result of a single optimizer tick.
pub struct TickResult {
    pub decisions: Vec<Decision>,
    pub applied: bool,
    pub rationale: String,
    pub assessment_before: RiskAssessment,
    pub assessment_after: RiskAssessment,
}

impl YieldOptimizer {
    pub fn new(
        portfolio: Portfolio,
        strategy: Box<dyn Strategy>,
        risk_model: RiskModel,
        risk_tolerance: RiskTolerance,
        max_var_fraction: f64,
    ) -> Self {
        let manager = PortfolioManager::new(portfolio, strategy, risk_tolerance);
        Self {
            manager,
            risk_model,
            max_var_fraction,
            rejected_count: 0,
            applied_count: 0,
        }
    }

    /// Run one optimization tick.
    ///
    /// Clones the portfolio, tries the strategy on the clone, checks the risk
    /// budget, and only applies the decisions to the real portfolio if safe.
    pub fn tick(&mut self, pool: &PoolSnapshot) -> TickResult {
        let assessment_before = self.risk_model.assess(&self.manager.portfolio);

        // Clone the portfolio and try the strategy on it
        let trial_portfolio = self.manager.portfolio.clone();
        // We need a reference to the underlying strategy; clone the portfolio and run it
        // through a trial manager that shares the same strategy box.
        // Since we can't clone Box<dyn Strategy>, we use a helper approach:
        // call strategy.decide() manually on the trial, apply decisions, then assess.
        let trial_decisions = self.manager.strategy.decide(
            &trial_portfolio,
            pool,
            &self.manager.risk,
        );

        // Apply trial decisions to a cloned portfolio
        let mut trial_managed = PortfolioManager::new(
            trial_portfolio,
            // We can't share the strategy here — create a dummy one that returns the cached decisions
            Box::new(CachedDecisions(trial_decisions.clone())),
            self.manager.risk.clone(),
        );
        trial_managed.tick(pool);

        let assessment_after = self.risk_model.assess(&trial_managed.portfolio);

        // Check the risk budget
        let passes = self.risk_model
            .passes_risk_budget(&trial_managed.portfolio, self.max_var_fraction)
            .unwrap_or(false);

        if passes {
            // Re-run on the real portfolio so positions actually update
            self.manager.tick(pool);
            self.applied_count += 1;
            TickResult {
                decisions: trial_decisions,
                applied: true,
                rationale: format!("risk budget OK: var/value <= {}", self.max_var_fraction),
                assessment_before,
                assessment_after,
            }
        } else {
            self.rejected_count += 1;
            TickResult {
                decisions: trial_decisions,
                applied: false,
                rationale: format!(
                    "REJECTED: post-decision VaR {} exceeds budget ({:.0}% of {})",
                    assessment_after.var_99_cu,
                    self.max_var_fraction * 100.0,
                    assessment_after.portfolio_value_cu
                ),
                assessment_before,
                assessment_after,
            }
        }
    }

    /// Reference to the real portfolio.
    pub fn portfolio(&self) -> &Portfolio {
        &self.manager.portfolio
    }

    /// Snapshot of current stats.
    pub fn stats(&self) -> HashMap<String, f64> {
        let mut m = self.manager.stats();
        m.insert("applied_ticks".into(), self.applied_count as f64);
        m.insert("rejected_ticks".into(), self.rejected_count as f64);
        m
    }
}

// ---------------------------------------------------------------------------
// CachedDecisions — internal helper strategy that returns a fixed decision list
// ---------------------------------------------------------------------------

struct CachedDecisions(Vec<Decision>);

impl Strategy for CachedDecisions {
    fn decide(
        &self,
        _portfolio: &Portfolio,
        _pool: &PoolSnapshot,
        _risk: &RiskTolerance,
    ) -> Vec<Decision> {
        self.0.clone()
    }

    fn name(&self) -> &str {
        "CachedDecisions"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::risk::RiskModel;
    use crate::strategies::HighYieldStrategy;
    use crate::types::{Portfolio, RiskTolerance};

    fn make_pool(reserve_ratio: f64) -> PoolSnapshot {
        let total = 1_000_000u64;
        let lent = (total as f64 * (1.0 - reserve_ratio)) as u64;
        let avail = (total as f64 * reserve_ratio) as u64;
        PoolSnapshot::new(total, lent, avail, reserve_ratio, 10, 0.003, 0, 0.005).unwrap()
    }

    #[test]
    fn test_yield_optimizer_applies_decisions_when_safe() {
        let portfolio = Portfolio::new(10_000);
        let mut opt = YieldOptimizer::new(
            portfolio,
            Box::new(HighYieldStrategy::new(0.3).unwrap()),
            RiskModel::new(0.001, 0.5, 2.33).unwrap(),
            RiskTolerance::Balanced,
            0.50,
        );
        let result = opt.tick(&make_pool(0.7));
        assert!(result.applied);
        assert_eq!(opt.applied_count, 1);
        assert!(opt.portfolio().total_lent() > 0);
    }

    #[test]
    fn test_yield_optimizer_rejects_decisions_when_var_exceeded() {
        let portfolio = Portfolio::new(10_000);
        // Extreme risk model + tight budget
        let mut opt = YieldOptimizer::new(
            portfolio,
            Box::new(HighYieldStrategy::new(1.0).unwrap()),
            RiskModel::new(0.5, 1.0, 2.33).unwrap(),
            RiskTolerance::Aggressive,
            0.05,
        );
        let result = opt.tick(&make_pool(0.8));
        assert!(!result.applied);
        assert_eq!(opt.rejected_count, 1);
        // Portfolio unchanged
        assert_eq!(opt.portfolio().total_lent(), 0);
        assert_eq!(opt.portfolio().cash_trm, 10_000);
    }

    #[test]
    fn test_yield_optimizer_stats() {
        let portfolio = Portfolio::new(10_000);
        let mut opt = YieldOptimizer::new(
            portfolio,
            Box::new(HighYieldStrategy::new(0.2).unwrap()),
            RiskModel::new(0.01, 0.5, 2.33).unwrap(),
            RiskTolerance::Balanced,
            0.20,
        );
        opt.tick(&make_pool(0.7));
        opt.tick(&make_pool(0.7));
        let stats = opt.stats();
        assert!(stats["applied_ticks"] >= 1.0);
        assert!(stats.contains_key("lent_cu"));
    }
}
