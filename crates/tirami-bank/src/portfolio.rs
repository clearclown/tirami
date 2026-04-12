//! PortfolioManager — tracks positions and applies decisions.
//!
//! The manager owns a Portfolio and a Strategy. On each `tick()` it asks the
//! strategy what to do given the current pool snapshot, then applies the
//! decisions to the portfolio. Decision execution is in-memory.
//!
//! Ported from the Python scaffold `tirami_bank/portfolio.py`.

use crate::strategies::Strategy;
use crate::types::{ActionKind, Decision, PoolSnapshot, Portfolio, Position, PositionKind, RiskTolerance};

/// Owns a portfolio and applies strategy decisions to it.
pub struct PortfolioManager {
    pub portfolio: Portfolio,
    pub strategy: Box<dyn Strategy>,
    pub risk: RiskTolerance,
    pub decision_history: Vec<(PoolSnapshot, Vec<Decision>)>,
}

impl PortfolioManager {
    pub fn new(portfolio: Portfolio, strategy: Box<dyn Strategy>, risk: RiskTolerance) -> Self {
        Self {
            portfolio,
            strategy,
            risk,
            decision_history: Vec::new(),
        }
    }

    /// Run one strategy step against the current pool snapshot.
    pub fn tick(&mut self, pool: &PoolSnapshot) -> Vec<Decision> {
        let decisions = self.strategy.decide(&self.portfolio, pool, &self.risk);
        self.decision_history.push((pool.clone(), decisions.clone()));
        for decision in &decisions {
            self.apply(decision);
        }
        decisions
    }

    /// Apply a single decision to the portfolio in memory.
    pub fn apply(&mut self, decision: &Decision) {
        match decision.action {
            ActionKind::Hold => {}

            ActionKind::Lend => {
                if self.portfolio.cash_trm < decision.trm_amount {
                    return; // silently no-op
                }
                self.portfolio.cash_trm -= decision.trm_amount;
                self.portfolio.positions.push(Position::simple(PositionKind::Lent, decision.trm_amount));
            }

            ActionKind::Borrow => {
                self.portfolio.cash_trm += decision.trm_amount;
                self.portfolio.positions.push(Position::simple(PositionKind::Borrowed, decision.trm_amount));
            }

            ActionKind::Repay => {
                // Find a borrowed position to repay
                if let Some(idx) = self
                    .portfolio
                    .positions
                    .iter()
                    .position(|p| p.kind == PositionKind::Borrowed && p.trm_amount == decision.trm_amount)
                {
                    self.portfolio.cash_trm -= decision.trm_amount;
                    self.portfolio.positions.remove(idx);
                }
            }

            // OPEN_FUTURES, CLOSE_FUTURES, BUY_INSURANCE — portfolio records intent but doesn't apply state
            ActionKind::OpenFutures | ActionKind::CloseFutures | ActionKind::BuyInsurance => {}
        }
    }

    /// Number of ticks executed so far.
    pub fn tick_count(&self) -> usize {
        self.decision_history.len()
    }

    /// Snapshot of current portfolio stats.
    pub fn stats(&self) -> std::collections::HashMap<String, f64> {
        let mut m = std::collections::HashMap::new();
        m.insert("ticks".into(), self.tick_count() as f64);
        m.insert("cash_trm".into(), self.portfolio.cash_trm as f64);
        m.insert("lent_cu".into(), self.portfolio.total_lent() as f64);
        m.insert("borrowed_cu".into(), self.portfolio.total_borrowed() as f64);
        m.insert("net_exposure_cu".into(), self.portfolio.net_cu_exposure() as f64);
        m.insert("position_count".into(), self.portfolio.positions.len() as f64);
        m
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::HighYieldStrategy;
    use crate::types::{ActionKind, Portfolio, Position, PositionKind, RiskTolerance};

    fn make_pool(reserve_ratio: f64) -> PoolSnapshot {
        let total = 1_000_000u64;
        let lent = (total as f64 * (1.0 - reserve_ratio)) as u64;
        let avail = (total as f64 * reserve_ratio) as u64;
        PoolSnapshot::new(total, lent, avail, reserve_ratio, 10, 0.003, 0, 0.005).unwrap()
    }

    #[test]
    fn test_lend_decision_reduces_cash_and_creates_position() {
        let portfolio = Portfolio::new(10_000);
        let mut mgr = PortfolioManager::new(
            portfolio,
            Box::new(HighYieldStrategy::new(0.5).unwrap()),
            RiskTolerance::Aggressive,
        );
        let pool = make_pool(0.6);
        mgr.tick(&pool);
        // 0.5 * 1.0 (aggressive) * 10000 = 5000 lent
        assert_eq!(mgr.portfolio.total_lent(), 5_000);
        assert_eq!(mgr.portfolio.cash_trm, 5_000);
    }

    #[test]
    fn test_borrow_decision_increases_cash_and_creates_position() {
        let portfolio = Portfolio::new(1_000);
        let mut mgr = PortfolioManager::new(
            portfolio,
            Box::new(HighYieldStrategy::default()),
            RiskTolerance::Balanced,
        );
        let decision = Decision::make(ActionKind::Borrow, 2_000, "test", 0.5);
        mgr.apply(&decision);
        assert_eq!(mgr.portfolio.cash_trm, 3_000);
        assert_eq!(mgr.portfolio.total_borrowed(), 2_000);
    }

    #[test]
    fn test_hold_does_nothing() {
        let portfolio = Portfolio::new(10_000);
        let mut mgr = PortfolioManager::new(
            portfolio,
            Box::new(HighYieldStrategy::default()),
            RiskTolerance::Balanced,
        );
        let decision = Decision::make(ActionKind::Hold, 0, "test", 0.5);
        mgr.apply(&decision);
        assert_eq!(mgr.portfolio.cash_trm, 10_000);
        assert!(mgr.portfolio.positions.is_empty());
    }

    #[test]
    fn test_lend_silently_no_op_if_insufficient_cash() {
        let portfolio = Portfolio::new(100);
        let mut mgr = PortfolioManager::new(
            portfolio,
            Box::new(HighYieldStrategy::default()),
            RiskTolerance::Balanced,
        );
        let decision = Decision::make(ActionKind::Lend, 1_000, "test", 0.5);
        mgr.apply(&decision);
        assert_eq!(mgr.portfolio.cash_trm, 100);
        assert_eq!(mgr.portfolio.total_lent(), 0);
    }

    #[test]
    fn test_tick_history_records_each_call() {
        let portfolio = Portfolio::new(10_000);
        let mut mgr = PortfolioManager::new(
            portfolio,
            Box::new(HighYieldStrategy::default()),
            RiskTolerance::Balanced,
        );
        mgr.tick(&make_pool(0.6));
        mgr.tick(&make_pool(0.5));
        mgr.tick(&make_pool(0.4));
        assert_eq!(mgr.tick_count(), 3);
    }

    #[test]
    fn test_stats_reflect_state() {
        let portfolio = Portfolio {
            cash_trm: 5_000,
            positions: vec![
                Position::simple(PositionKind::Lent, 3_000),
                Position::simple(PositionKind::Borrowed, 1_000),
            ],
        };
        let mgr = PortfolioManager::new(
            portfolio,
            Box::new(HighYieldStrategy::default()),
            RiskTolerance::Balanced,
        );
        let stats = mgr.stats();
        assert_eq!(stats["cash_trm"] as u64, 5_000);
        assert_eq!(stats["lent_cu"] as u64, 3_000);
        assert_eq!(stats["borrowed_cu"] as u64, 1_000);
        // net_exposure = cash + lent - borrowed = 5000 + 3000 - 1000 = 7000
        assert_eq!(stats["net_exposure_cu"] as i64, 7_000);
    }

    #[test]
    fn test_portfolio_net_exposure_includes_collateral() {
        let portfolio = Portfolio {
            cash_trm: 1_000,
            positions: vec![
                Position::simple(PositionKind::Lent, 2_000),
                Position::simple(PositionKind::Collateral, 500),
                Position::simple(PositionKind::Borrowed, 1_500),
            ],
        };
        // 1000 + 2000 + 500 - 1500 = 2000
        assert_eq!(portfolio.net_cu_exposure(), 2_000);
    }
}
