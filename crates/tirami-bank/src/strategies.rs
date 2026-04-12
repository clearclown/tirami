//! Lending strategies.
//!
//! A strategy is a pure function: given the agent's portfolio, the current
//! pool snapshot, and a risk tolerance, return a list of recommended
//! decisions. Strategies do NOT execute decisions — that's the
//! PortfolioManager's job.
//!
//! Ported from the Python scaffold `tirami_bank/strategies.py`.

use serde::{Deserialize, Serialize};

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
        let max_commit = (portfolio.cash_trm as f64 * self.max_commit_fraction).floor() as u64;
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

/// Default base commit fraction for HighYieldStrategy.
/// Matches forge-economics/spec/parameters.md §10.2 `highyield_base_commit_fraction`.
pub const DEFAULT_HIGHYIELD_COMMIT_FRACTION: f64 = 0.50;

impl Default for HighYieldStrategy {
    fn default() -> Self {
        Self::new(DEFAULT_HIGHYIELD_COMMIT_FRACTION).unwrap()
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
            let target = (portfolio.cash_trm as f64 * commit_fraction).floor() as u64;
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
            let borrow_target = pool.your_max_borrow_cu.min(portfolio.cash_trm / 2);
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
// StrategyKind — serializable discriminant for persistence
// ---------------------------------------------------------------------------

/// Serializable discriminant for a lending strategy.
///
/// Used by the persistence layer to snapshot/restore a `Box<dyn Strategy>`
/// without requiring trait-object serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StrategyKind {
    Conservative { max_commit_fraction: f64 },
    HighYield { base_commit_fraction: f64 },
    Balanced { threshold: f64 },
}

impl Default for StrategyKind {
    fn default() -> Self {
        Self::Balanced { threshold: 0.50 }
    }
}

impl StrategyKind {
    /// Reconstruct a heap-allocated strategy from this discriminant.
    ///
    /// Returns `Err` only if the discriminant holds an out-of-range parameter
    /// (e.g. a fraction > 1.0 loaded from a corrupt snapshot file).
    pub fn to_strategy(&self) -> Result<Box<dyn Strategy>, BankError> {
        match self {
            Self::Conservative { max_commit_fraction } => {
                ConservativeStrategy::new(*max_commit_fraction).map(|s| Box::new(s) as Box<dyn Strategy>)
            }
            Self::HighYield { base_commit_fraction } => {
                HighYieldStrategy::new(*base_commit_fraction).map(|s| Box::new(s) as Box<dyn Strategy>)
            }
            Self::Balanced { threshold } => {
                Ok(Box::new(BalancedStrategy::new(*threshold)))
            }
        }
    }

    /// Infer the StrategyKind from a live strategy's name and current parameters.
    pub fn from_strategy(strategy: &dyn Strategy) -> Self {
        match strategy.name() {
            "ConservativeStrategy" => {
                // Downcast is not possible without Any; use the default fraction.
                // The snapshot captures the fraction from BankServices directly.
                Self::Conservative { max_commit_fraction: 0.30 }
            }
            "HighYieldStrategy" => Self::HighYield { base_commit_fraction: DEFAULT_HIGHYIELD_COMMIT_FRACTION },
            _ => Self::default(),
        }
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
        assert_eq!(lend.unwrap().trm_amount, 3_000); // 30% of 10k
    }

    #[test]
    fn test_conservative_respects_existing_lent_position() {
        let s = ConservativeStrategy::new(0.3).unwrap();
        let portfolio = Portfolio {
            cash_trm: 10_000,
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
        assert_eq!(lend_decisions[0].trm_amount, 7_000); // 70% * 1.0 risk multiplier
    }

    #[test]
    fn test_high_yield_scales_with_risk_tolerance() {
        let s = HighYieldStrategy::new(0.7).unwrap();
        let pool = default_pool(0.5);

        let cons = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Conservative);
        let bal = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
        let aggr = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Aggressive);

        let cons_lend = cons.iter().find(|d| d.action == ActionKind::Lend).unwrap().trm_amount;
        let bal_lend = bal.iter().find(|d| d.action == ActionKind::Lend).unwrap().trm_amount;
        let aggr_lend = aggr.iter().find(|d| d.action == ActionKind::Lend).unwrap().trm_amount;

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

    // ===========================================================================
    // DEEP SECURITY TESTS — Round 2 (NaN/Inf pool fields, zero-cash, edge cases)
    // ===========================================================================

    #[test]
    fn sec_deep_conservative_with_nan_reserve_ratio_does_not_panic() {
        // PoolSnapshot::new validates reserve_ratio in [0,1], so NaN is rejected.
        // Verify the constructor rejects NaN without panic.
        let result = crate::types::PoolSnapshot::new(100_000, 0, 100_000, f64::NAN, 0, 0.0, 0, 0.0);
        assert!(result.is_err(), "NaN reserve_ratio must be rejected by PoolSnapshot::new");
    }

    #[test]
    fn sec_deep_conservative_with_infinity_reserve_ratio_does_not_panic() {
        let result = crate::types::PoolSnapshot::new(100_000, 0, 100_000, f64::INFINITY, 0, 0.0, 0, 0.0);
        assert!(result.is_err(), "Infinity reserve_ratio must be rejected");
    }

    #[test]
    fn sec_deep_strategy_zero_cash_all_hold() {
        // Portfolio with 0 CU cash — strategies must return Hold, not panic.
        let s_conservative = ConservativeStrategy::default();
        let s_high = HighYieldStrategy::default();
        let s_balanced = BalancedStrategy::default();
        let pool = default_pool(0.7);
        let empty = Portfolio::new(0);

        for (name, decisions) in [
            ("conservative", s_conservative.decide(&empty, &pool, &RiskTolerance::Balanced)),
            ("high_yield",   s_high.decide(&empty, &pool, &RiskTolerance::Balanced)),
            ("balanced",     s_balanced.decide(&empty, &pool, &RiskTolerance::Balanced)),
        ] {
            assert!(
                !decisions.is_empty(),
                "{name} must return at least one decision for zero-cash portfolio"
            );
            // With zero cash, Lend amount should be 0 (nothing to lend).
            for d in &decisions {
                if d.action == ActionKind::Lend {
                    assert_eq!(d.trm_amount, 0, "{name}: Lend trm_amount must be 0 when portfolio cash is 0");
                }
            }
        }
    }

    #[test]
    fn sec_deep_strategy_kind_conservative_roundtrip() {
        let kind = StrategyKind::Conservative { max_commit_fraction: 0.25 };
        let strategy = kind.to_strategy().expect("must produce valid strategy");
        assert_eq!(strategy.name(), "ConservativeStrategy");
    }

    #[test]
    fn sec_deep_strategy_kind_high_yield_roundtrip() {
        let kind = StrategyKind::HighYield { base_commit_fraction: 0.60 };
        let strategy = kind.to_strategy().expect("must produce valid strategy");
        assert_eq!(strategy.name(), "HighYieldStrategy");
    }

    #[test]
    fn sec_deep_strategy_kind_balanced_roundtrip() {
        let kind = StrategyKind::Balanced { threshold: 0.45 };
        let strategy = kind.to_strategy().expect("must produce valid strategy");
        assert_eq!(strategy.name(), "BalancedStrategy");
    }

    #[test]
    fn sec_deep_strategy_kind_invalid_fraction_rejected() {
        // Out-of-range fraction must return Err, not panic.
        let kind = StrategyKind::Conservative { max_commit_fraction: 2.0 };
        assert!(kind.to_strategy().is_err(), "fraction > 1.0 must be rejected");

        let kind2 = StrategyKind::HighYield { base_commit_fraction: 0.0 };
        assert!(kind2.to_strategy().is_err(), "fraction = 0.0 must be rejected");
    }

    #[test]
    fn sec_deep_high_yield_infinity_avg_interest_rate_does_not_panic() {
        // avg_interest_rate = INF in a pool snapshot that passes validation.
        // PoolSnapshot::new does not validate avg_interest_rate, so INF may be stored.
        let pool_result = crate::types::PoolSnapshot::new(
            1_000_000, 0, 1_000_000, 0.8, 10, f64::INFINITY, 0, f64::INFINITY,
        );
        if let Ok(pool) = pool_result {
            let s = HighYieldStrategy::default();
            // Must not panic regardless of INF fields.
            let decisions = s.decide(&Portfolio::new(10_000), &pool, &RiskTolerance::Balanced);
            assert!(!decisions.is_empty(), "strategy must produce decisions for INF-rate pool");
        }
        // If PoolSnapshot::new rejected it, that's also acceptable.
    }
}
