//! Loan insurance.
//!
//! A simple point-to-point insurance contract: an insurer agrees to cover
//! borrower default on a specific loan in exchange for a CU premium paid
//! upfront.
//!
//! Ported from the Python scaffold `tirami_bank/insurance.py`.

use serde::{Deserialize, Serialize};

use crate::errors::BankError;

// ---------------------------------------------------------------------------
// InsurancePolicy
// ---------------------------------------------------------------------------

/// Coverage against borrower default on a specific loan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InsurancePolicy {
    pub policy_id: String,
    /// NodeId of the party providing coverage.
    pub insurer_hex: String,
    /// NodeId of the party buying protection (typically the lender).
    pub insured_party_hex: String,
    /// Hex-encoded loan_id from forge.
    pub insured_loan_id_hex: String,
    /// Maximum CU paid out on default.
    pub coverage_trm: u64,
    /// Upfront premium paid by insured to insurer.
    pub premium_cu: u64,
    /// Timestamp at which coverage ends.
    pub expires_at_ms: u64,
}

impl InsurancePolicy {
    /// Construct an InsurancePolicy, validating all invariants.
    pub fn new(
        policy_id: impl Into<String>,
        insurer_hex: impl Into<String>,
        insured_party_hex: impl Into<String>,
        insured_loan_id_hex: impl Into<String>,
        coverage_trm: u64,
        premium_cu: u64,
        expires_at_ms: u64,
    ) -> Result<Self, BankError> {
        let insurer_hex = insurer_hex.into();
        let insured_party_hex = insured_party_hex.into();

        if coverage_trm == 0 {
            return Err(BankError::InvalidParameter(
                "coverage_trm must be > 0".into(),
            ));
        }
        if premium_cu == 0 {
            return Err(BankError::InvalidParameter(
                "premium_cu must be > 0".into(),
            ));
        }
        if coverage_trm < premium_cu {
            return Err(BankError::InvalidParameter(
                "coverage_trm must be >= premium_cu".into(),
            ));
        }
        if insurer_hex.len() != 64 || insured_party_hex.len() != 64 {
            return Err(BankError::InvalidParameter(
                "party hex must be 64 chars".into(),
            ));
        }

        Ok(Self {
            policy_id: policy_id.into(),
            insurer_hex,
            insured_party_hex,
            insured_loan_id_hex: insured_loan_id_hex.into(),
            coverage_trm,
            premium_cu,
            expires_at_ms,
        })
    }
}

// ---------------------------------------------------------------------------
// premium_for
// ---------------------------------------------------------------------------

/// Calculate the insurance premium in CU.
///
/// Premium scales inversely with the borrower's credit score:
/// ```text
/// rate = base_rate + (1 - credit_score) * risk_premium
/// premium = max(1, floor(coverage * rate))
/// ```
///
/// # Examples
/// - credit 1.0, coverage 1000 → premium = 1000 * 0.02 = 20 CU
/// - credit 0.5, coverage 1000 → premium = 1000 * 0.07 = 70 CU
/// - credit 0.3, coverage 1000 → premium = 1000 * 0.09 = 90 CU
pub fn premium_for(
    coverage_trm: u64,
    borrower_credit_score: f64,
    base_rate: f64,
    risk_premium: f64,
) -> Result<u64, BankError> {
    if coverage_trm == 0 {
        return Err(BankError::InvalidParameter(
            "coverage_trm must be > 0".into(),
        ));
    }
    if !(0.0..=1.0).contains(&borrower_credit_score) {
        return Err(BankError::InvalidParameter(format!(
            "borrower_credit_score must be in [0, 1], got {borrower_credit_score}"
        )));
    }
    if base_rate < 0.0 || risk_premium < 0.0 {
        return Err(BankError::InvalidParameter(
            "rates must be non-negative".into(),
        ));
    }

    let credit = borrower_credit_score.clamp(0.0, 1.0);
    let rate = base_rate + (1.0 - credit) * risk_premium;
    let raw = (coverage_trm as f64 * rate).floor() as u64;
    Ok(raw.max(1))
}

/// `premium_for` with default rates (base=0.02, risk=0.10).
pub fn premium_for_default(coverage_trm: u64, borrower_credit_score: f64) -> Result<u64, BankError> {
    premium_for(coverage_trm, borrower_credit_score, 0.02, 0.10)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn hex64(seed: &str) -> String {
        seed.repeat(64)[..64].to_string()
    }

    fn make_policy(coverage_trm: u64, premium_cu: u64) -> InsurancePolicy {
        InsurancePolicy::new(
            "p1",
            hex64("a"),
            hex64("b"),
            "c".repeat(64),
            coverage_trm,
            premium_cu,
            1_700_000_000_000,
        )
        .unwrap()
    }

    #[test]
    fn test_insurance_policy_validates_amounts() {
        let result = InsurancePolicy::new(
            "p1",
            hex64("a"),
            hex64("b"),
            "c".repeat(64),
            0,
            10,
            1_000,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_insurance_policy_rejects_premium_above_coverage() {
        let result = InsurancePolicy::new(
            "p1",
            hex64("a"),
            hex64("b"),
            "c".repeat(64),
            100,
            200,
            1_000,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_insurance_policy_round_trip() {
        let p = make_policy(1_000, 50);
        assert_eq!(p.coverage_trm, 1_000);
        assert_eq!(p.premium_cu, 50);
    }

    #[test]
    fn test_premium_lowest_for_perfect_credit() {
        let p_perfect = premium_for_default(1_000, 1.0).unwrap();
        let p_zero = premium_for_default(1_000, 0.0).unwrap();
        assert!(p_perfect < p_zero);
    }

    #[test]
    fn test_premium_default_rates_at_perfect_credit() {
        // base_rate=0.02, coverage=1000 → 20
        let p = premium_for_default(1_000, 1.0).unwrap();
        assert_eq!(p, 20);
    }

    #[test]
    fn test_premium_default_rates_at_zero_credit() {
        // 0.02 + 1.0 * 0.10 = 0.12, coverage=1000 → 120
        let p = premium_for_default(1_000, 0.0).unwrap();
        assert_eq!(p, 120);
    }

    #[test]
    fn test_premium_validates_credit_score() {
        assert!(premium_for_default(1_000, 1.5).is_err());
        assert!(premium_for_default(1_000, -0.1).is_err());
    }

    #[test]
    fn test_premium_minimum_one_cu() {
        // Tiny coverage shouldn't round down to 0 — at least 1 CU
        let p = premium_for_default(1, 1.0).unwrap();
        assert!(p >= 1);
    }

    #[test]
    fn test_premium_validates_coverage() {
        assert!(premium_for_default(0, 0.5).is_err());
    }

    // ===========================================================================
    // Security tests — insurance economic attack vectors
    // ===========================================================================

    #[test]
    fn sec_premium_for_rejects_zero_coverage() {
        // Zero coverage is meaningless and could seed a division-by-zero.
        let result = premium_for_default(0, 0.5);
        assert!(result.is_err(), "zero coverage_trm must be rejected");
    }

    #[test]
    fn sec_premium_for_rejects_credit_above_one() {
        // credit_score > 1.0 is out-of-range and must be rejected.
        let result = premium_for_default(1_000, 1.5);
        assert!(
            result.is_err(),
            "credit score 1.5 must be rejected (out of [0,1])"
        );
    }

    #[test]
    fn sec_premium_for_rejects_negative_credit() {
        // credit_score < 0.0 is out-of-range and must be rejected.
        let result = premium_for_default(1_000, -0.1);
        assert!(
            result.is_err(),
            "negative credit score must be rejected"
        );
    }

    #[test]
    fn sec_insurance_policy_rejects_zero_coverage() {
        // An InsurancePolicy with coverage = 0 is invalid.
        let result = InsurancePolicy::new(
            "p-zero",
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
            0,   // zero coverage
            1,
            1_000_000,
        );
        assert!(result.is_err(), "InsurancePolicy with zero coverage must be rejected");
    }

    #[test]
    fn sec_insurance_policy_rejects_zero_premium() {
        // A policy that charges zero premium gives away free protection.
        let result = InsurancePolicy::new(
            "p-free",
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
            1_000,
            0,   // zero premium
            1_000_000,
        );
        assert!(result.is_err(), "InsurancePolicy with zero premium must be rejected");
    }

    #[test]
    fn sec_insurance_premium_increases_with_lower_credit() {
        // Premium is risk-adjusted: lower credit must produce higher premium.
        let high_credit = premium_for_default(10_000, 0.9).unwrap();
        let low_credit = premium_for_default(10_000, 0.1).unwrap();
        assert!(
            low_credit > high_credit,
            "lower credit score must produce higher premium: {} vs {}",
            low_credit,
            high_credit
        );
    }

    #[test]
    fn sec_insurance_premium_never_zero_for_any_valid_coverage() {
        // Even perfect credit with minimal coverage must yield at least 1 CU premium.
        for coverage in [1u64, 2, 5, 10, 100] {
            let p = premium_for_default(coverage, 1.0).unwrap();
            assert!(
                p >= 1,
                "premium must be at least 1 CU for coverage={}, got {}",
                coverage,
                p
            );
        }
    }
}
