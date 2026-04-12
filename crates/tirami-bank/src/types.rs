//! Core data types for forge-bank.
//!
//! Ported from the Python scaffold `tirami_bank/types.py`.

use serde::{Deserialize, Serialize};

use crate::errors::BankError;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The kind of financial position held.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionKind {
    /// CU offered to lending pool.
    Lent,
    /// CU received as a loan.
    Borrowed,
    /// CU locked as collateral against a loan.
    Collateral,
    /// Forward purchase of CU at strike.
    FuturesLong,
    /// Forward sale of CU at strike.
    FuturesShort,
    /// Loan covered by an insurance policy.
    Insured,
}

/// A strategy's recommended action.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Lend,
    Borrow,
    Repay,
    Hold,
    OpenFutures,
    CloseFutures,
    BuyInsurance,
}

/// Configurable risk appetite for strategies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTolerance {
    /// Prioritize capital preservation.
    Conservative,
    /// Mix of growth and safety.
    Balanced,
    /// Maximize yield, accept volatility.
    Aggressive,
}

// ---------------------------------------------------------------------------
// PoolSnapshot
// ---------------------------------------------------------------------------

/// Current state of a Forge node's lending pool, fetched via forge-sdk.
///
/// Mirrors the JSON shape returned by `GET /v1/tirami/pool`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolSnapshot {
    pub total_trm: u64,
    pub lent_cu: u64,
    pub available_cu: u64,
    pub reserve_ratio: f64,
    pub active_loan_count: u64,
    pub avg_interest_rate: f64,
    pub your_max_borrow_cu: u64,
    pub your_offered_rate: f64,
}

impl PoolSnapshot {
    /// Create a new `PoolSnapshot`, validating inputs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        total_trm: u64,
        lent_cu: u64,
        available_cu: u64,
        reserve_ratio: f64,
        active_loan_count: u64,
        avg_interest_rate: f64,
        your_max_borrow_cu: u64,
        your_offered_rate: f64,
    ) -> Result<Self, BankError> {
        if !(0.0..=1.0).contains(&reserve_ratio) {
            return Err(BankError::InvalidParameter(format!(
                "reserve_ratio must be in [0.0, 1.0], got {reserve_ratio}"
            )));
        }
        Ok(Self {
            total_trm,
            lent_cu,
            available_cu,
            reserve_ratio,
            active_loan_count,
            avg_interest_rate,
            your_max_borrow_cu,
            your_offered_rate,
        })
    }

    /// Fraction of the pool currently lent out.
    pub fn utilization(&self) -> f64 {
        if self.total_trm == 0 {
            0.0
        } else {
            self.lent_cu as f64 / self.total_trm as f64
        }
    }
}

// ---------------------------------------------------------------------------
// Position
// ---------------------------------------------------------------------------

/// A single financial position held by the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub kind: PositionKind,
    pub trm_amount: u64,
    pub counterparty_id_hex: Option<String>,
    pub metadata: serde_json::Value,
}

impl Position {
    /// Create a new position, validating that `trm_amount > 0`.
    pub fn new(kind: PositionKind, trm_amount: u64) -> Result<Self, BankError> {
        if trm_amount == 0 {
            return Err(BankError::InvalidParameter(
                "position trm_amount must be > 0".into(),
            ));
        }
        Ok(Self {
            kind,
            trm_amount,
            counterparty_id_hex: None,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        })
    }

    /// Create a position (panics on invalid input — for internal use).
    pub(crate) fn simple(kind: PositionKind, trm_amount: u64) -> Self {
        Self::new(kind, trm_amount).expect("trm_amount must be > 0")
    }
}

// ---------------------------------------------------------------------------
// Portfolio
// ---------------------------------------------------------------------------

/// All positions held by an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Portfolio {
    pub cash_trm: u64,
    pub positions: Vec<Position>,
}

impl Portfolio {
    /// Create a portfolio with the given starting cash.
    pub fn new(cash_trm: u64) -> Self {
        Self {
            cash_trm,
            positions: Vec::new(),
        }
    }

    /// Sum of all LENT positions.
    pub fn total_lent(&self) -> u64 {
        self.positions
            .iter()
            .filter(|p| p.kind == PositionKind::Lent)
            .map(|p| p.trm_amount)
            .sum()
    }

    /// Sum of all BORROWED positions.
    pub fn total_borrowed(&self) -> u64 {
        self.positions
            .iter()
            .filter(|p| p.kind == PositionKind::Borrowed)
            .map(|p| p.trm_amount)
            .sum()
    }

    /// Sum of all COLLATERAL positions.
    pub fn total_collateral(&self) -> u64 {
        self.positions
            .iter()
            .filter(|p| p.kind == PositionKind::Collateral)
            .map(|p| p.trm_amount)
            .sum()
    }

    /// `cash + lent + collateral - borrowed`.
    ///
    /// Returns a signed integer because the result can be negative in theory
    /// (e.g. borrowed > cash + lent + collateral).
    pub fn net_cu_exposure(&self) -> i64 {
        let positive = self.cash_trm as i64 + self.total_lent() as i64 + self.total_collateral() as i64;
        let negative = self.total_borrowed() as i64;
        positive - negative
    }
}

// ---------------------------------------------------------------------------
// Decision
// ---------------------------------------------------------------------------

/// A strategy's recommendation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub action: ActionKind,
    /// For HOLD decisions this is 0; for all others it must be > 0.
    pub trm_amount: u64,
    pub rationale: String,
    pub confidence: f64,
}

impl Decision {
    /// Create a Decision, validating confidence and trm_amount rules.
    pub fn new(
        action: ActionKind,
        trm_amount: u64,
        rationale: impl Into<String>,
        confidence: f64,
    ) -> Result<Self, BankError> {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(BankError::InvalidParameter(format!(
                "confidence must be in [0.0, 1.0], got {confidence}"
            )));
        }
        if action != ActionKind::Hold && trm_amount == 0 {
            return Err(BankError::InvalidParameter(format!(
                "non-HOLD decision must have positive trm_amount, got {trm_amount}"
            )));
        }
        Ok(Self {
            action,
            trm_amount,
            rationale: rationale.into(),
            confidence,
        })
    }

    /// Convenience constructor; panics on invalid inputs (for internal strategy use).
    pub(crate) fn make(
        action: ActionKind,
        trm_amount: u64,
        rationale: impl Into<String>,
        confidence: f64,
    ) -> Self {
        Self::new(action, trm_amount, rationale, confidence).expect("Decision::make invalid args")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pool(reserve_ratio: f64) -> PoolSnapshot {
        let total = 1_000_000u64;
        let lent = (total as f64 * (1.0 - reserve_ratio)) as u64;
        let avail = (total as f64 * reserve_ratio) as u64;
        PoolSnapshot::new(total, lent, avail, reserve_ratio, 10, 0.003, 0, 0.005).unwrap()
    }

    #[test]
    fn test_pool_snapshot_validates_reserve_ratio() {
        assert!(PoolSnapshot::new(100, 0, 100, -0.1, 0, 0.0, 0, 0.0).is_err());
        assert!(PoolSnapshot::new(100, 0, 100, 1.5, 0, 0.0, 0, 0.0).is_err());
        assert!(PoolSnapshot::new(100, 0, 100, 0.5, 0, 0.0, 0, 0.0).is_ok());
    }

    #[test]
    fn test_pool_utilization_zero_when_empty() {
        let pool = PoolSnapshot::new(0, 0, 0, 0.0, 0, 0.0, 0, 0.0).unwrap();
        assert_eq!(pool.utilization(), 0.0);
    }

    #[test]
    fn test_pool_utilization_correct() {
        let pool = make_pool(0.6);
        // lent = 400_000, total = 1_000_000 → 0.4
        let expected = 400_000.0 / 1_000_000.0;
        assert!((pool.utilization() - expected).abs() < 1e-9);
    }

    #[test]
    fn test_position_validates_nonzero_amount() {
        assert!(Position::new(PositionKind::Lent, 0).is_err());
        assert!(Position::new(PositionKind::Lent, 1).is_ok());
    }

    #[test]
    fn test_portfolio_totals() {
        let portfolio = Portfolio {
            cash_trm: 1_000,
            positions: vec![
                Position::simple(PositionKind::Lent, 2_000),
                Position::simple(PositionKind::Borrowed, 500),
                Position::simple(PositionKind::Collateral, 300),
            ],
        };
        assert_eq!(portfolio.total_lent(), 2_000);
        assert_eq!(portfolio.total_borrowed(), 500);
        assert_eq!(portfolio.total_collateral(), 300);
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

    #[test]
    fn test_decision_validates_confidence() {
        assert!(Decision::new(ActionKind::Hold, 0, "ok", 1.5).is_err());
        assert!(Decision::new(ActionKind::Hold, 0, "ok", -0.1).is_err());
        assert!(Decision::new(ActionKind::Hold, 0, "ok", 0.5).is_ok());
    }

    #[test]
    fn test_decision_non_hold_requires_positive_amount() {
        assert!(Decision::new(ActionKind::Lend, 0, "fail", 0.5).is_err());
        assert!(Decision::new(ActionKind::Lend, 100, "ok", 0.5).is_ok());
    }
}
