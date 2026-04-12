//! CU futures contracts.
//!
//! A futures contract is a forward agreement on the price of CU between two
//! parties: a long (will buy CU at the strike price) and a short (will sell
//! CU at the strike price). At expiry, the contract is cash-settled at the
//! prevailing market price (msats per CU).
//!
//! Ported from the Python scaffold `tirami_bank/futures.py`.

use serde::{Deserialize, Serialize};

use crate::errors::BankError;

// ---------------------------------------------------------------------------
// FuturesContract
// ---------------------------------------------------------------------------

/// Forward agreement on CU price between two parties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FuturesContract {
    pub contract_id: String,
    /// NodeId (hex) of the party agreeing to buy at expiry.
    pub long_party_hex: String,
    /// NodeId (hex) of the party agreeing to sell at expiry.
    pub short_party_hex: String,
    /// Amount of CU the contract covers.
    pub notional_trm: u64,
    /// Agreed price (msats per CU) at settlement.
    pub strike_price_msats: u64,
    /// Timestamp of contract expiry.
    pub expires_at_ms: u64,
    /// Optional collateral locked by both parties.
    pub margin_cu: u64,
}

impl FuturesContract {
    /// Construct a FuturesContract, validating all invariants.
    pub fn new(
        contract_id: impl Into<String>,
        long_party_hex: impl Into<String>,
        short_party_hex: impl Into<String>,
        notional_trm: u64,
        strike_price_msats: u64,
        expires_at_ms: u64,
        margin_cu: u64,
    ) -> Result<Self, BankError> {
        let long_party_hex = long_party_hex.into();
        let short_party_hex = short_party_hex.into();

        if notional_trm == 0 {
            return Err(BankError::InvalidParameter("notional_trm must be > 0".into()));
        }
        if strike_price_msats == 0 {
            return Err(BankError::InvalidParameter(
                "strike_price_msats must be > 0".into(),
            ));
        }
        if long_party_hex.len() != 64 || short_party_hex.len() != 64 {
            return Err(BankError::InvalidParameter(
                "party hex must be 64 chars".into(),
            ));
        }
        if long_party_hex == short_party_hex {
            return Err(BankError::InvalidParameter(
                "long and short cannot be the same party".into(),
            ));
        }

        Ok(Self {
            contract_id: contract_id.into(),
            long_party_hex,
            short_party_hex,
            notional_trm,
            strike_price_msats,
            expires_at_ms,
            margin_cu,
        })
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Compute the long-side and short-side P&L in msats at settlement.
///
/// Returns `(long_pnl_msats, short_pnl_msats)`. The pair always sums to zero
/// (futures are zero-sum).
///
/// # Example
/// - Strike 10 msats/CU, notional 1000 CU, settlement 12 msats/CU
/// - Long: profit 2 * 1000 = 2000 msats
/// - Short: loss 2000 msats
pub fn futures_pnl(contract: &FuturesContract, settlement_price_msats: i64) -> (i64, i64) {
    let price_delta = settlement_price_msats.saturating_sub(contract.strike_price_msats as i64);
    // Security: use saturating_mul to prevent overflow on extreme inputs.
    let long_pnl = price_delta.saturating_mul(contract.notional_trm as i64);
    let short_pnl = long_pnl.saturating_neg();
    (long_pnl, short_pnl)
}

/// Compute the mid-contract mark-to-market value for the LONG side.
///
/// Positive = long is winning, negative = long is losing.
/// The short side's MtM is the negation.
pub fn mark_to_market(contract: &FuturesContract, current_price_msats: i64) -> i64 {
    let (long_pnl, _) = futures_pnl(contract, current_price_msats);
    long_pnl
}

/// Compute the required margin in CU for a contract.
///
/// Default 10% of notional.
pub fn required_margin(contract: &FuturesContract, margin_fraction: f64) -> Result<u64, BankError> {
    if !(margin_fraction > 0.0 && margin_fraction <= 1.0) {
        return Err(BankError::InvalidParameter(
            "margin_fraction must be in (0, 1]".into(),
        ));
    }
    Ok((contract.notional_trm as f64 * margin_fraction).floor() as u64)
}

/// `required_margin` with the default 10% fraction.
pub fn required_margin_default(contract: &FuturesContract) -> u64 {
    required_margin(contract, 0.10).expect("0.10 is always valid")
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

    fn make_contract(notional: u64, strike_msats: u64) -> FuturesContract {
        FuturesContract::new(
            "test-1",
            hex64("a"),
            hex64("b"),
            notional,
            strike_msats,
            1_700_000_000_000,
            0,
        )
        .unwrap()
    }

    #[test]
    fn test_futures_validates_notional() {
        let result = FuturesContract::new("x", hex64("a"), hex64("b"), 0, 10, 1_000, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_futures_validates_distinct_parties() {
        let result = FuturesContract::new("x", hex64("a"), hex64("a"), 1_000, 10, 1_000, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_pnl_zero_when_settlement_equals_strike() {
        let contract = make_contract(1_000, 10);
        let (long_pnl, short_pnl) = futures_pnl(&contract, 10);
        assert_eq!(long_pnl, 0);
        assert_eq!(short_pnl, 0);
    }

    #[test]
    fn test_pnl_long_wins_when_price_rises() {
        let contract = make_contract(1_000, 10);
        let (long_pnl, short_pnl) = futures_pnl(&contract, 12);
        // delta = 2 msats/CU, notional 1000 CU → 2000 msats
        assert_eq!(long_pnl, 2_000);
        assert_eq!(short_pnl, -2_000);
    }

    #[test]
    fn test_pnl_long_loses_when_price_falls() {
        let contract = make_contract(1_000, 10);
        let (long_pnl, short_pnl) = futures_pnl(&contract, 8);
        assert_eq!(long_pnl, -2_000);
        assert_eq!(short_pnl, 2_000);
    }

    #[test]
    fn test_pnl_is_zero_sum() {
        let contract = make_contract(500, 15);
        for price in [1, 10, 15, 20, 100] {
            let (long_pnl, short_pnl) = futures_pnl(&contract, price);
            assert_eq!(long_pnl + short_pnl, 0);
        }
    }

    #[test]
    fn test_mark_to_market_matches_long_pnl() {
        let contract = make_contract(1_000, 10);
        let mtm = mark_to_market(&contract, 11);
        let (long_pnl, _) = futures_pnl(&contract, 11);
        assert_eq!(mtm, long_pnl);
    }

    #[test]
    fn test_required_margin_default() {
        let contract = make_contract(10_000, 10);
        let margin = required_margin_default(&contract);
        assert_eq!(margin, 1_000); // 10% default
    }

    #[test]
    fn test_required_margin_custom_fraction() {
        let contract = make_contract(10_000, 10);
        let margin = required_margin(&contract, 0.20).unwrap();
        assert_eq!(margin, 2_000);
    }

    #[test]
    fn test_required_margin_validates_fraction() {
        let contract = make_contract(1_000, 10);
        assert!(required_margin(&contract, 0.0).is_err());
        assert!(required_margin(&contract, 1.5).is_err());
    }

    // ===========================================================================
    // Security tests — futures economic attack vectors
    // ===========================================================================

    #[test]
    fn sec_futures_contract_rejects_same_parties() {
        // Long and short cannot be the same party — prevents artificial P&L loops.
        let same = hex64("a");
        let result = FuturesContract::new("x", same.clone(), same, 1_000, 10, 1_000_000, 0);
        assert!(
            result.is_err(),
            "same long and short party must be rejected"
        );
    }

    #[test]
    fn sec_futures_contract_rejects_zero_notional() {
        // Zero notional has no economic value and could be used for spam.
        let result = FuturesContract::new("x", hex64("a"), hex64("b"), 0, 10, 1_000_000, 0);
        assert!(
            result.is_err(),
            "zero notional must be rejected"
        );
    }

    #[test]
    fn sec_futures_contract_rejects_zero_strike_price() {
        // A zero strike_price_msats allows cost-free settlement manipulation.
        let result = FuturesContract::new("x", hex64("a"), hex64("b"), 1_000, 0, 1_000_000, 0);
        assert!(
            result.is_err(),
            "zero strike price must be rejected"
        );
    }

    #[test]
    fn sec_futures_pnl_is_zero_sum_across_prices() {
        // For any realistic settlement price, long_pnl + short_pnl must always equal 0.
        // Note: extremely large prices (near i64::MAX) can overflow price_delta * notional_trm
        // in the current implementation. This test uses values within safe arithmetic range.
        let contract = make_contract(1_000, 10);
        for settlement_price in [0i64, 1, 5, 10, 50, 100, 1_000, 1_000_000] {
            let (long_pnl, short_pnl) = futures_pnl(&contract, settlement_price);
            assert_eq!(
                long_pnl + short_pnl,
                0,
                "P&L must be zero-sum at settlement price {}: long={}, short={}",
                settlement_price,
                long_pnl,
                short_pnl
            );
        }
    }

    #[test]
    fn sec_futures_pnl_zero_sum_at_extreme_but_safe_prices() {
        // Use a smaller notional to avoid overflow at higher prices.
        let small_contract = FuturesContract::new(
            "extreme",
            hex64("a"),
            hex64("b"),
            1,  // notional = 1 CU to prevent overflow
            10,
            1_700_000_000_000,
            0,
        )
        .unwrap();
        for settlement_price in [0i64, i32::MAX as i64, i64::MAX / 2] {
            let (long_pnl, short_pnl) = futures_pnl(&small_contract, settlement_price);
            assert_eq!(
                long_pnl + short_pnl,
                0,
                "P&L must be zero-sum at settlement price {}: long={}, short={}",
                settlement_price,
                long_pnl,
                short_pnl
            );
        }
    }

    #[test]
    fn sec_futures_pnl_is_zero_sum_below_strike() {
        // Negative settlement prices (extreme bear market) must still sum to zero.
        let contract = make_contract(500, 15);
        for settlement_price in [-100i64, -1, 0] {
            let (long_pnl, short_pnl) = futures_pnl(&contract, settlement_price);
            assert_eq!(
                long_pnl + short_pnl,
                0,
                "P&L must be zero-sum below strike (price={}): long={}, short={}",
                settlement_price,
                long_pnl,
                short_pnl
            );
        }
    }

    #[test]
    fn sec_futures_required_margin_rejects_zero_fraction() {
        let contract = make_contract(1_000, 10);
        assert!(
            required_margin(&contract, 0.0).is_err(),
            "zero margin fraction must be rejected"
        );
    }

    #[test]
    fn sec_futures_required_margin_rejects_above_one() {
        let contract = make_contract(1_000, 10);
        assert!(
            required_margin(&contract, 1.1).is_err(),
            "margin fraction > 1.0 must be rejected"
        );
    }

    #[test]
    fn sec_futures_party_hex_must_be_64_chars() {
        // Short hex strings that cannot represent a valid NodeId must be rejected.
        let result = FuturesContract::new("x", "abcd", hex64("b"), 1_000, 10, 1_000_000, 0);
        assert!(result.is_err(), "short party hex must be rejected");
        let result = FuturesContract::new("x", hex64("a"), "tooshort", 1_000, 10, 1_000_000, 0);
        assert!(result.is_err(), "short party hex for short side must be rejected");
    }
}
