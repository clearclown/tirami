//! Lending strategies.
//!
//! A strategy is a pure function: given the agent's portfolio, the current
//! pool snapshot, and a risk tolerance, return a list of recommended
//! decisions. Strategies do NOT execute decisions — that's the
//! PortfolioManager's job.
//!
//! Ported from the Python scaffold `forge_bank/strategies.py`.

use crate::errors::BankError;
use crate::types::{ActionKind, Decision, PoolSnapshot, Portfolio, RiskTolerance};

// ---------------------------------------------------------------------------
// Strategy trait
// ---------------------------------------------------------------------------

/// Abstract strategy that produces decisions given portfolio state.
pub trait Strategy: Send + Sync {
    fn decide(
        &self,
        portfolio: &Portfolio,
        pool: &PoolSnapshot,
        risk: &RiskTolerance,
    ) -> Vec<Decision>;

    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// ConservativeStrategy
// ---------------------------------------------------------------------------

/// Capital preservation: lend only when pool is over-reserved.
///
/// - LEND only when `reserve_ratio > 0.6`
/// - HOLD if pool is at or near minimum reserve
/// - Never borrow
/// - Maximum `max_commit_fraction` of cash committed at any time
pub struct ConservativeStrategy {
    pub max_commit_fraction: f64,
}

impl ConservativeStrategy {
    pub fn new(max_commit_fraction: f64) -> Result<Self, BankError> {
        if !(max_commit_fraction > 0.0 && max_commit_fraction <= 1.0) {
            return Err(BankError::InvalidParameter(
                "max_commit_fraction must be in (0, 1]".into(),
            ));
        }
        Ok(Self { max_commit_fraction })
    }
}

impl Default for ConservativeStrategy {
    fn default() -> Self {
        Self::new(0.30).unwrap()
    }
}

impl Strategy for ConservativeStrategy {
    fn decide(
        &self,
        portfolio: &Portfolio,
        pool: &PoolSnapshot,
        _risk: &RiskTolerance,
    ) -> Vec<Decision> {
        if pool.reserve_ratio < 0.6 {
            return vec![Decision::make(
                ActionKind::Hold,
                0,
                format!(
                    "pool reserve {:.2} below conservative threshold 0.60",
                    pool.reserve_ratio
                ),
                0.9,
            )];
        }

        let already_lent = portfolio.total_lent();
        let max_commit = (portfolio.cash_cu as f64 * self.max_commit_fraction).floor() as u64;
        let room = max_commit.saturating_sub(already_lent);

        if room == 0 {
            return vec![Decision::make(
                ActionKind::Hold,
                0,
                format!(
                    "already at conservative cap ({:.0}%)",
                    self.max_commit_fraction * 100.0
                ),
                0.95,
            )];
        }

        vec![Decision::make(
            ActionKind::Lend,
            room,
            format!(
                "pool over-reserved at {:.2}; lend {} CU",
                pool.reserve_ratio, room
            ),
            0.7,
        )]
    }

    fn name(&self) -> &str {
        "ConservativeStrategy"
    }
}

// ---------------------------------------------------------------------------
// HighYieldStrategy
// ---------------------------------------------------------------------------

/// Yield-seeking: lend aggressively, borrow when rates favorable.
///
/// - LEND large fraction when `reserve_ratio > 0.4`
/// - BORROW when offered rate < 0.002 and credit allows
/// - Risk tolerance scales the commit fractions
pub struct HighYieldStrategy {
    pub base_commit_fraction: f64,
}

impl HighYieldStrategy {
    pub fn new(base_commit_fraction: f64) -> Result<Self, BankError> {
        if !(base_commit_fraction > 0.0 && base_commit_fraction <= 1.0) {
            return Err(BankError::InvalidParameter(
                "base_commit_fraction must be in (0, 1]".into(),
            ));
        }
        Ok(Self { base_commit_fraction })
    }
}

impl Default for HighYieldStrategy {
    fn default() -> Self {
        Self::new(0.70).unwrap()
    }
}

impl Strategy for HighYieldStrategy {
    fn decide(
        &self,
        portfolio: &Portfolio,
        pool: &PoolSnapshot,
        risk: &RiskTolerance,
    ) -> Vec<Decision> {
        let risk_multiplier = match risk {
            RiskTolerance::Conservative => 0.5,
            RiskTolerance::Balanced => 0.8,
            RiskTolerance::Aggressive => 1.0,
        };
        let commit_fraction = self.base_commit_fraction * risk_multiplier;

        let mut decisions: Vec<Decision> = Vec::new();

        // Lend when pool has room
        if pool.reserve_ratio > 0.4 {
            let already_lent = portfolio.total_lent();
            let target = (portfolio.cash_cu as f64 * commit_fraction).floor() as u64;
            let room = target.saturating_sub(already_lent);
            if room > 0 {
                decisions.push(Decision::make(
                    ActionKind::Lend,
                    room,
                    format!("high-yield: lending {:.0}% of cash", commit_fraction * 100.0),
                    0.75,
                ));
            }
        }

        // Borrow when rates are very low
        if pool.your_offered_rate < 0.002 && pool.your_max_borrow_cu > 0 {
            let borrow_target = pool.your_max_borrow_cu.min(portfolio.cash_cu / 2);
            if borrow_target > 0 {
                decisions.push(Decision::make(
                    ActionKind::Borrow,
                    borrow_target,
                    format!(
                        "rate {:.4} below threshold; borrow",
                        pool.your_offered_rate
                    ),
                    0.6,
                ));
            }
        }

        if decisions.is_empty() {
            vec![Decision::make(
                ActionKind::Hold,
                0,
                "no high-yield opportunities right now",
                0.5,
            )]
        } else {
            decisions
        }
    }

    fn name(&self) -> &str {
        "HighYieldStrategy"
    }
}

// ---------------------------------------------------------------------------
// BalancedStrategy
// ---------------------------------------------------------------------------

/// Mix of conservative and high-yield based on pool conditions.
///
/// Routes to `ConservativeStrategy` when pool reserve is tight,
/// `HighYieldStrategy` when pool is healthy.
pub struct BalancedStrategy {
    pub threshold: f64,
    conservative: ConservativeStrategy,
    high_yield: HighYieldStrategy,
}

impl BalancedStrategy {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            conservative: ConservativeStrategy::default(),
            high_yield: HighYieldStrategy::default(),
        }
    }
}

impl Default for BalancedStrategy {
    fn default() -> Self {
        Self::new(0.50)
    }
}

impl Strategy for BalancedStrategy {
    fn decide(
        &self,
        portfolio: &Portfolio,
        pool: &PoolSnapshot,
        risk: &RiskTolerance,
    ) -> Vec<Decision> {
        if pool.reserve_ratio < self.threshold {
            self.conservative.decide(portfolio, pool, risk)
        } else {
            self.high_yield.decide(portfolio, pool, risk)
        }
    }

    fn name(&self) -> &str {
        "BalancedStrategy"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Portfolio, Position, PositionKind};

    fn make_pool(reserve_ratio: f64, your_offered_rate: f64, your_max_borrow_cu: u64) -> PoolSnapshot {
        let total = 1_000_000u64;
        let lent = (total as f64 * (1.0 - reserve_ratio)) as u64;
        let avail = (total as f64 * reserve_ratio) as u64;
        PoolSnapshot::new(total, lent, avail, reserve_ratio, 10, 0.003, your_max_borrow_cu, your_offered_rate).unwrap()
    }

    fn default_pool(reserve_ratio: f64) -> PoolSnapshot {
        make_pool(reserve_ratio, 0.005, 0)
    }

    // ---------- ConservativeStrategy ----------

    #[test]
    fn test_conservative_holds_when_pool_tight() {
        let s = ConservativeStrategy::default();
        let pool = default_pool(0.4); // below 0.6 threshold
        let decisions = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].action, ActionKind::Hold);
    }

    #[test]
    fn test_conservative_lends_when_pool_over_reserved() {
        let s = ConservativeStrategy::new(0.3).unwrap();
        let pool = default_pool(0.7);
        let decisions = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
        let lend = decisions.iter().find(|d| d.action == ActionKind::Lend);
        assert!(lend.is_some());
        assert_eq!(lend.unwrap().cu_amount, 3_000); // 30% of 10k
    }

    #[test]
    fn test_conservative_respects_existing_lent_position() {
        let s = ConservativeStrategy::new(0.3).unwrap();
        let portfolio = Portfolio {
            cash_cu: 10_000,
            positions: vec![Position::simple(PositionKind::Lent, 3_000)],
        };
        let pool = default_pool(0.7);
        let decisions = s.decide(&portfolio, &pool, &RiskTolerance::Balanced);
        // Already at cap → HOLD
        assert_eq!(decisions[0].action, ActionKind::Hold);
    }

    #[test]
    fn test_conservative_validates_commit_fraction() {
        assert!(ConservativeStrategy::new(1.5).is_err());
        assert!(ConservativeStrategy::new(0.0).is_err());
    }

    // ---------- HighYieldStrategy ----------

    #[test]
    fn test_high_yield_lends_aggressively_when_pool_healthy() {
        let s = HighYieldStrategy::new(0.7).unwrap();
        let pool = default_pool(0.5);
        let decisions = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Aggressive);
        let lend_decisions: Vec<_> = decisions.iter().filter(|d| d.action == ActionKind::Lend).collect();
        assert_eq!(lend_decisions.len(), 1);
        assert_eq!(lend_decisions[0].cu_amount, 7_000); // 70% * 1.0 risk multiplier
    }

    #[test]
    fn test_high_yield_scales_with_risk_tolerance() {
        let s = HighYieldStrategy::new(0.7).unwrap();
        let pool = default_pool(0.5);

        let cons = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Conservative);
        let bal = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
        let aggr = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Aggressive);

        let cons_lend = cons.iter().find(|d| d.action == ActionKind::Lend).unwrap().cu_amount;
        let bal_lend = bal.iter().find(|d| d.action == ActionKind::Lend).unwrap().cu_amount;
        let aggr_lend = aggr.iter().find(|d| d.action == ActionKind::Lend).unwrap().cu_amount;

        assert!(cons_lend < bal_lend);
        assert!(bal_lend < aggr_lend);
    }

    #[test]
    fn test_high_yield_borrows_when_rate_low() {
        let s = HighYieldStrategy::default();
        let pool = make_pool(0.5, 0.001, 5_000); // rate below threshold
        let decisions = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
        assert!(decisions.iter().any(|d| d.action == ActionKind::Borrow));
    }

    #[test]
    fn test_high_yield_holds_when_no_opportunities() {
        let s = HighYieldStrategy::default();
        let pool = make_pool(0.2, 0.01, 0); // too tight to lend, too high to borrow
        let decisions = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].action, ActionKind::Hold);
    }

    // ---------- BalancedStrategy ----------

    #[test]
    fn test_balanced_routes_to_conservative_when_tight() {
        let s = BalancedStrategy::new(0.50);
        let pool = default_pool(0.40);
        let decisions = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
        // Conservative refuses to lend below 0.6 → HOLD
        assert_eq!(decisions[0].action, ActionKind::Hold);
    }

    #[test]
    fn test_balanced_routes_to_high_yield_when_healthy() {
        let s = BalancedStrategy::new(0.50);
        let pool = default_pool(0.55);
        let decisions = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
        // HighYield will lend
        assert!(decisions.iter().any(|d| d.action == ActionKind::Lend));
    }
}
