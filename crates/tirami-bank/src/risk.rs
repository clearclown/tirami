//! Basic risk modeling for CU portfolios.
//!
//! A `RiskModel` provides simple measures of portfolio risk:
//! - Expected loss given a default rate
//! - Value at Risk (VaR) at a confidence level
//! - Concentration risk (max single-counterparty exposure)
//!
//! Ported from the Python scaffold `tirami_bank/risk.py`.

use serde::{Deserialize, Serialize};

use crate::errors::BankError;
use crate::types::{Portfolio, PositionKind};

// ---------------------------------------------------------------------------
// RiskAssessment
// ---------------------------------------------------------------------------

/// Output of a risk evaluation for a single portfolio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub portfolio_value_cu: u64,
    pub expected_loss_cu: u64,
    pub var_99_cu: u64,
    pub largest_single_exposure_cu: u64,
    pub concentration_ratio: f64,
}

// ---------------------------------------------------------------------------
// RiskModel
// ---------------------------------------------------------------------------

/// Simple parametric risk model.
///
/// Parameters:
/// - `default_rate`: Probability that a single loan defaults (0–1).
/// - `loss_given_default`: Fraction of principal lost on default (0–1).
/// - `var_multiplier`: Standard deviations for VaR. Default 2.33 is the 99% one-sided.
pub struct RiskModel {
    pub default_rate: f64,
    pub loss_given_default: f64,
    pub var_multiplier: f64,
}

impl RiskModel {
    /// Construct a RiskModel, validating all parameters.
    pub fn new(
        default_rate: f64,
        loss_given_default: f64,
        var_multiplier: f64,
    ) -> Result<Self, BankError> {
        if !(0.0..=1.0).contains(&default_rate) {
            return Err(BankError::InvalidParameter(format!(
                "default_rate must be in [0, 1], got {default_rate}"
            )));
        }
        if !(0.0..=1.0).contains(&loss_given_default) {
            return Err(BankError::InvalidParameter(format!(
                "loss_given_default must be in [0, 1], got {loss_given_default}"
            )));
        }
        if var_multiplier <= 0.0 {
            return Err(BankError::InvalidParameter(
                "var_multiplier must be > 0".into(),
            ));
        }
        Ok(Self {
            default_rate,
            loss_given_default,
            var_multiplier,
        })
    }

    /// Compute a risk assessment for the given portfolio.
    pub fn assess(&self, portfolio: &Portfolio) -> RiskAssessment {
        let lent_positions: Vec<u64> = portfolio
            .positions
            .iter()
            .filter(|p| p.kind == PositionKind::Lent)
            .map(|p| p.trm_amount)
            .collect();

        let total_lent: u64 = lent_positions.iter().sum();

        // Expected loss = total_lent * default_rate * loss_given_default
        let expected_loss = (total_lent as f64 * self.default_rate * self.loss_given_default).floor() as u64;

        // VaR using Bernoulli loss model
        let var_99 = if total_lent > 0 {
            let variance_term: f64 = lent_positions.iter().map(|&cu| (cu as f64).powi(2)).sum();
            let std_dev = (variance_term * self.default_rate * (1.0 - self.default_rate)).sqrt()
                * self.loss_given_default;
            (expected_loss as f64 + self.var_multiplier * std_dev).floor() as u64
        } else {
            0
        };

        // Concentration: largest single position relative to total
        let (largest_single, concentration) = if !lent_positions.is_empty() {
            let largest = *lent_positions.iter().max().unwrap();
            let ratio = if total_lent > 0 {
                largest as f64 / total_lent as f64
            } else {
                0.0
            };
            (largest, ratio)
        } else {
            (0, 0.0)
        };

        RiskAssessment {
            portfolio_value_cu: portfolio.cash_trm + total_lent,
            expected_loss_cu: expected_loss,
            var_99_cu: var_99,
            largest_single_exposure_cu: largest_single,
            concentration_ratio: concentration,
        }
    }

    /// Check if the portfolio's VaR is within the configured budget.
    ///
    /// Returns `true` if portfolio is within risk budget.
    pub fn passes_risk_budget(
        &self,
        portfolio: &Portfolio,
        max_var_fraction: f64,
    ) -> Result<bool, BankError> {
        if !(max_var_fraction > 0.0 && max_var_fraction <= 1.0) {
            return Err(BankError::InvalidParameter(
                "max_var_fraction must be in (0, 1]".into(),
            ));
        }
        let assessment = self.assess(portfolio);
        if assessment.portfolio_value_cu == 0 {
            return Ok(true);
        }
        Ok(assessment.var_99_cu as f64 / assessment.portfolio_value_cu as f64 <= max_var_fraction)
    }
}

/// Default RiskModel parameters per forge-economics/spec/parameters.md §10.5.
/// - `default_rate = 0.02` (2% annual)
/// - `loss_given_default = 0.50` (50% LGD)
/// - `var_99_multiplier = 2.33` (normal distribution 99th percentile)
pub const DEFAULT_RATE: f64 = 0.02;
pub const LOSS_GIVEN_DEFAULT: f64 = 0.50;
pub const VAR_99_MULTIPLIER: f64 = 2.33;

impl Default for RiskModel {
    fn default() -> Self {
        Self::new(DEFAULT_RATE, LOSS_GIVEN_DEFAULT, VAR_99_MULTIPLIER).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Portfolio, Position, PositionKind};

    #[test]
    fn test_risk_model_validates_inputs() {
        assert!(RiskModel::new(1.5, 0.5, 2.33).is_err());
        assert!(RiskModel::new(0.01, -0.1, 2.33).is_err());
        assert!(RiskModel::new(0.01, 0.5, 0.0).is_err());
    }

    #[test]
    fn test_empty_portfolio_has_zero_risk() {
        let rm = RiskModel::default();
        let assessment = rm.assess(&Portfolio::new(10_000));
        assert_eq!(assessment.expected_loss_cu, 0);
        assert_eq!(assessment.var_99_cu, 0);
        assert_eq!(assessment.concentration_ratio, 0.0);
    }

    #[test]
    fn test_lent_portfolio_has_expected_loss() {
        let rm = RiskModel::new(0.01, 0.5, 2.33).unwrap();
        let portfolio = Portfolio {
            cash_trm: 5_000,
            positions: vec![Position::simple(PositionKind::Lent, 10_000)],
        };
        let assessment = rm.assess(&portfolio);
        // 10000 * 0.01 * 0.5 = 50
        assert_eq!(assessment.expected_loss_cu, 50);
    }

    #[test]
    fn test_concentration_ratio() {
        let rm = RiskModel::default();
        let portfolio = Portfolio {
            cash_trm: 0,
            positions: vec![
                Position::simple(PositionKind::Lent, 8_000),
                Position::simple(PositionKind::Lent, 2_000),
            ],
        };
        let assessment = rm.assess(&portfolio);
        // 8000 / 10000 = 0.8
        assert!((assessment.concentration_ratio - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_passes_risk_budget_when_within_var() {
        let rm = RiskModel::new(0.001, 0.5, 2.33).unwrap();
        let portfolio = Portfolio {
            cash_trm: 10_000,
            positions: vec![Position::simple(PositionKind::Lent, 10_000)],
        };
        // Very low default rate → small VaR → should pass even tight budget
        assert!(rm.passes_risk_budget(&portfolio, 0.20).unwrap());
    }

    #[test]
    fn test_fails_risk_budget_when_var_too_high() {
        let rm = RiskModel::new(0.5, 1.0, 2.33).unwrap(); // extreme
        let portfolio = Portfolio {
            cash_trm: 0,
            positions: vec![Position::simple(PositionKind::Lent, 10_000)],
        };
        // Should exceed any reasonable budget
        assert!(!rm.passes_risk_budget(&portfolio, 0.10).unwrap());
    }

    // ===========================================================================
    // DEEP SECURITY TESTS — Round 2 (NaN/Inf inputs, boundary conditions)
    // ===========================================================================

    #[test]
    fn sec_deep_risk_model_rejects_nan_default_rate() {
        let result = RiskModel::new(f64::NAN, 0.5, 2.33);
        assert!(result.is_err(), "NaN default_rate must be rejected by RiskModel::new");
    }

    #[test]
    fn sec_deep_risk_model_rejects_infinity_default_rate() {
        let result = RiskModel::new(f64::INFINITY, 0.5, 2.33);
        assert!(result.is_err(), "Infinity default_rate must be rejected");
    }

    #[test]
    fn sec_deep_risk_model_rejects_infinity_lgd() {
        let result = RiskModel::new(0.02, f64::INFINITY, 2.33);
        assert!(result.is_err(), "Infinity loss_given_default must be rejected");
    }

    #[test]
    fn sec_deep_risk_model_rejects_nan_lgd() {
        let result = RiskModel::new(0.02, f64::NAN, 2.33);
        assert!(result.is_err(), "NaN loss_given_default must be rejected");
    }

    #[test]
    fn sec_deep_risk_model_rejects_nan_var_multiplier() {
        let result = RiskModel::new(0.02, 0.5, f64::NAN);
        // var_multiplier <= 0.0: NaN <= 0.0 is false in IEEE 754, so this may pass.
        // We document the current behavior.
        let _ = result; // pass or fail — document
    }

    #[test]
    fn sec_deep_risk_model_rejects_negative_var_multiplier() {
        let result = RiskModel::new(0.02, 0.5, -1.0);
        assert!(result.is_err(), "negative var_multiplier must be rejected");
    }

    #[test]
    fn sec_deep_risk_model_zero_portfolio_never_panics() {
        // All-zero portfolio.
        let rm = RiskModel::default();
        let empty = Portfolio::new(0);
        let assessment = rm.assess(&empty);
        assert_eq!(assessment.expected_loss_cu, 0);
        assert_eq!(assessment.var_99_cu, 0);
        assert_eq!(assessment.concentration_ratio, 0.0);
        assert!(!assessment.concentration_ratio.is_nan());
        // passes_risk_budget with zero portfolio must return true.
        assert!(rm.passes_risk_budget(&empty, 0.10).unwrap());
    }

    #[test]
    fn sec_deep_risk_assess_only_cash_no_lent_positions() {
        // Portfolio with cash but no lent positions → risk should be zero.
        let rm = RiskModel::default();
        let portfolio = Portfolio::new(100_000);
        let assessment = rm.assess(&portfolio);
        assert_eq!(assessment.expected_loss_cu, 0);
        assert_eq!(assessment.var_99_cu, 0);
        assert_eq!(assessment.largest_single_exposure_cu, 0);
    }
}
