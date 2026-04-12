//! CU budget management for self-improvement.
//!
//! A `TrmBudget` is a hard cap on how much the self-improvement loop can spend.
//! It tracks per-cycle and per-day spending, and gates each spend with a
//! predicate.

use serde::{Deserialize, Serialize};

/// Spending policy for the forge-mind self-improvement loop.
///
/// All limits are HARD limits — the loop will not spend even one CU above
/// the policy, even if it would yield a great improvement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrmBudget {
    // Per-cycle limits
    pub max_trm_per_cycle: u64,

    // Per-day limits
    pub max_trm_per_day: u64,
    pub max_cycles_per_day: u32,

    // Quality gates — improvements below these thresholds are reverted.
    /// Must improve by 1% absolute
    pub min_score_delta: f64,
    /// Must pay back > 1x what it cost
    pub min_roi_threshold: f64,

    // Internal counters
    pub spent_today_cu: u64,
    pub cycles_today: u32,
    pub day_started_at_ms: u64,
}

impl Default for TrmBudget {
    fn default() -> Self {
        Self {
            max_trm_per_cycle: 5_000,
            max_trm_per_day: 50_000,
            max_cycles_per_day: 20,
            min_score_delta: 0.01,
            min_roi_threshold: 1.0,
            spent_today_cu: 0,
            cycles_today: 0,
            day_started_at_ms: 0,
        }
    }
}

impl TrmBudget {
    /// Reset per-day counters if 24 hours have elapsed.
    ///
    /// Returns `true` if a reset happened.
    pub fn maybe_reset_day(&mut self, now_ms: u64) -> bool {
        if now_ms.saturating_sub(self.day_started_at_ms) >= 24 * 3_600_000 {
            self.spent_today_cu = 0;
            self.cycles_today = 0;
            self.day_started_at_ms = now_ms;
            return true;
        }
        false
    }

    /// Check if another improvement cycle can start right now.
    pub fn can_start_cycle(&self) -> bool {
        self.cycles_today < self.max_cycles_per_day
    }

    /// Increment the cycle counter for the current day.
    ///
    /// Returns `Err` if the daily cycle limit has been reached.
    pub fn record_cycle_start(&mut self) -> Result<(), String> {
        if !self.can_start_cycle() {
            return Err("refused to start cycle — daily cycle limit reached".to_string());
        }
        self.cycles_today += 1;
        Ok(())
    }

    /// Check if a spend of this size is permitted right now.
    pub fn can_spend(&self, trm_amount: u64) -> bool {
        if trm_amount == 0 {
            return false;
        }
        if trm_amount > self.max_trm_per_cycle {
            return false;
        }
        if self.spent_today_cu + trm_amount > self.max_trm_per_day {
            return false;
        }
        if self.cycles_today >= self.max_cycles_per_day {
            return false;
        }
        true
    }

    /// Record that a spend occurred. Caller must check `can_spend` first.
    ///
    /// Returns `Err` if the spend would exceed the budget.
    pub fn record_spend(&mut self, trm_amount: u64) -> Result<(), String> {
        if !self.can_spend(trm_amount) {
            return Err(format!(
                "refused to record spend of {} CU — exceeds budget",
                trm_amount
            ));
        }
        self.spent_today_cu += trm_amount;
        Ok(())
    }

    /// Decide whether an improvement should be kept based on policy.
    pub fn is_improvement_worth_keeping(
        &self,
        score_delta: f64,
        trm_invested: u64,
        cu_return_estimate: u64,
    ) -> bool {
        if score_delta < self.min_score_delta {
            return false;
        }
        if trm_invested == 0 {
            return score_delta > 0.0; // Free improvement, always keep
        }
        let roi = cu_return_estimate as f64 / trm_invested as f64;
        roi >= self.min_roi_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_budget_allows_typical_spends() {
        let b = TrmBudget::default();
        assert!(b.can_spend(100));
        assert!(b.can_spend(b.max_trm_per_cycle));
    }

    #[test]
    fn test_can_spend_rejects_zero() {
        let b = TrmBudget::default();
        assert!(!b.can_spend(0));
    }

    #[test]
    fn test_can_spend_rejects_above_per_cycle_cap() {
        let b = TrmBudget {
            max_trm_per_cycle: 1_000,
            ..TrmBudget::default()
        };
        assert!(b.can_spend(1_000));
        assert!(!b.can_spend(1_001));
    }

    #[test]
    fn test_can_spend_rejects_above_per_day_cap() {
        let mut b = TrmBudget {
            max_trm_per_cycle: 10_000,
            max_trm_per_day: 10_000,
            ..TrmBudget::default()
        };
        b.record_spend(8_000).unwrap();
        assert!(b.can_spend(2_000));
        assert!(!b.can_spend(2_001));
    }

    #[test]
    fn test_record_spend_increments_counter() {
        let mut b = TrmBudget::default();
        b.record_spend(500).unwrap();
        assert_eq!(b.spent_today_cu, 500);
        b.record_spend(200).unwrap();
        assert_eq!(b.spent_today_cu, 700);
    }

    #[test]
    fn test_record_spend_refuses_when_over_budget() {
        let mut b = TrmBudget {
            max_trm_per_cycle: 100,
            max_trm_per_day: 100,
            ..TrmBudget::default()
        };
        b.record_spend(100).unwrap();
        assert!(b.record_spend(1).is_err());
    }

    #[test]
    fn test_cycle_count_limit() {
        let mut b = TrmBudget {
            max_cycles_per_day: 3,
            ..TrmBudget::default()
        };
        assert!(b.can_start_cycle());
        b.record_cycle_start().unwrap();
        b.record_cycle_start().unwrap();
        b.record_cycle_start().unwrap();
        assert!(!b.can_start_cycle());
        assert!(b.record_cycle_start().is_err());
    }

    #[test]
    fn test_day_rollover_resets_counters() {
        let mut b = TrmBudget {
            max_cycles_per_day: 2,
            ..TrmBudget::default()
        };
        b.record_cycle_start().unwrap();
        b.record_spend(500).unwrap();
        // Simulate 25 hours later
        let later = b.day_started_at_ms + 25 * 3_600_000;
        let reset = b.maybe_reset_day(later);
        assert!(reset);
        assert_eq!(b.cycles_today, 0);
        assert_eq!(b.spent_today_cu, 0);
        assert!(b.can_start_cycle());
    }

    #[test]
    fn test_day_rollover_no_op_within_24h() {
        let mut b = TrmBudget::default();
        b.record_spend(500).unwrap();
        let later = b.day_started_at_ms + 3_600_000; // 1 hour
        let reset = b.maybe_reset_day(later);
        assert!(!reset);
        assert_eq!(b.spent_today_cu, 500);
    }

    #[test]
    fn test_is_improvement_worth_keeping_rejects_low_delta() {
        let b = TrmBudget {
            min_score_delta: 0.05,
            ..TrmBudget::default()
        };
        assert!(!b.is_improvement_worth_keeping(0.01, 100, 1000));
    }

    #[test]
    fn test_is_improvement_worth_keeping_accepts_good_roi() {
        let b = TrmBudget {
            min_score_delta: 0.01,
            min_roi_threshold: 2.0,
            ..TrmBudget::default()
        };
        assert!(b.is_improvement_worth_keeping(0.05, 100, 300));
    }

    #[test]
    fn test_is_improvement_worth_keeping_rejects_low_roi() {
        let b = TrmBudget {
            min_score_delta: 0.01,
            min_roi_threshold: 2.0,
            ..TrmBudget::default()
        };
        assert!(!b.is_improvement_worth_keeping(0.05, 100, 150));
    }

    #[test]
    fn test_zero_investment_improvement_always_kept_if_delta_positive() {
        let b = TrmBudget::default();
        assert!(b.is_improvement_worth_keeping(0.1, 0, 0));
    }

    #[test]
    fn test_budget_rolls_over_after_24h() {
        let mut budget = TrmBudget::default();
        budget.record_spend(500).unwrap();
        assert_eq!(budget.spent_today_cu, 500);
        // 24h + 1ms later — rollover should reset counters
        let later = budget.day_started_at_ms + 24 * 3600 * 1000 + 1;
        let reset = budget.maybe_reset_day(later);
        assert!(reset);
        assert_eq!(budget.spent_today_cu, 0);
        assert_eq!(budget.cycles_today, 0);
        // After rollover, can spend again
        assert!(budget.can_spend(1_000));
    }

    #[test]
    fn test_can_spend_requires_cycle_capacity() {
        let mut b = TrmBudget {
            max_cycles_per_day: 1,
            ..TrmBudget::default()
        };
        // With 0 cycles used, can_spend should work (cycles_today < max)
        assert!(b.can_spend(100));
        // Once we use the cycle slot, can_spend returns false
        b.record_cycle_start().unwrap();
        assert!(!b.can_spend(100));
    }

    // ===========================================================================
    // Security tests — TrmBudget exhaustion and hard-limit enforcement
    // ===========================================================================

    #[test]
    fn sec_cu_budget_rejects_spend_over_per_cycle_limit() {
        // An AI agent must not be able to spend more than max_trm_per_cycle in one call.
        let b = TrmBudget {
            max_trm_per_cycle: 5_000,
            ..TrmBudget::default()
        };
        assert!(
            !b.can_spend(5_001),
            "spend of 5001 CU must be rejected when max_trm_per_cycle = 5000"
        );
        assert!(
            b.can_spend(5_000),
            "spend of exactly 5000 CU must be allowed at the limit"
        );
    }

    #[test]
    fn sec_cu_budget_rejects_spend_over_daily_limit() {
        // Accumulate near the daily limit, then one more spend must be blocked.
        let mut b = TrmBudget {
            max_trm_per_cycle: 50_000,
            max_trm_per_day: 50_000,
            max_cycles_per_day: 20,
            ..TrmBudget::default()
        };
        b.record_spend(49_000).unwrap();
        assert!(b.can_spend(1_000), "1000 CU fits in remaining daily budget");
        assert!(
            !b.can_spend(1_001),
            "1001 CU must be rejected: would exceed daily limit"
        );
        let result = b.record_spend(1_001);
        assert!(
            result.is_err(),
            "record_spend over daily limit must return Err, got {:?}",
            result
        );
    }

    #[test]
    fn sec_cu_budget_rejects_over_max_cycles_per_day() {
        // Once max_cycles_per_day is exhausted, no further cycles may start.
        let mut b = TrmBudget {
            max_cycles_per_day: 3,
            ..TrmBudget::default()
        };
        for _ in 0..3 {
            b.record_cycle_start().unwrap();
        }
        let result = b.record_cycle_start();
        assert!(
            result.is_err(),
            "starting a 4th cycle when max is 3 must return Err"
        );
        assert!(
            !b.can_start_cycle(),
            "can_start_cycle must return false when daily cycle limit is exhausted"
        );
    }

    #[test]
    fn sec_cu_budget_record_spend_enforces_per_cycle_cap() {
        // record_spend must also enforce the per-cycle cap (not only can_spend).
        let mut b = TrmBudget {
            max_trm_per_cycle: 100,
            max_trm_per_day: 10_000,
            max_cycles_per_day: 20,
            ..TrmBudget::default()
        };
        let result = b.record_spend(101);
        assert!(
            result.is_err(),
            "record_spend of 101 CU with per-cycle cap 100 must fail"
        );
    }

    #[test]
    fn sec_cu_budget_daily_reset_clears_exhausted_cycle_count() {
        // After day rollover, cycle count resets and spending resumes.
        let mut b = TrmBudget {
            max_cycles_per_day: 1,
            ..TrmBudget::default()
        };
        b.record_cycle_start().unwrap();
        assert!(!b.can_start_cycle(), "cycle limit reached");

        // Simulate 24h + 1ms rollover.
        let tomorrow = b.day_started_at_ms + 24 * 3_600_000 + 1;
        b.maybe_reset_day(tomorrow);

        assert!(
            b.can_start_cycle(),
            "after day rollover, cycle count must be reset"
        );
        assert_eq!(b.cycles_today, 0, "cycles_today must be 0 after reset");
        assert_eq!(b.spent_today_cu, 0, "spent_today_cu must be 0 after reset");
    }

    #[test]
    fn sec_cu_budget_zero_cu_spend_always_rejected() {
        // A zero-CU spend must always be rejected regardless of budget state.
        let b = TrmBudget::default();
        assert!(
            !b.can_spend(0),
            "zero-CU spend must always be rejected"
        );
    }

    // ===========================================================================
    // DEEP SECURITY TESTS — Round 2 (day-rollover timing, overflow, ROI edge cases)
    // ===========================================================================

    #[test]
    fn sec_deep_budget_day_rollover_at_exact_24h_boundary() {
        // Rollover must trigger at exactly 24 * 3_600_000 ms.
        let mut b = TrmBudget::default();
        let start = b.day_started_at_ms;
        b.record_spend(500).unwrap();

        let exact_boundary = start + 24 * 3_600_000;
        let reset = b.maybe_reset_day(exact_boundary);
        assert!(reset, "rollover must trigger at exactly 24h boundary");
        assert_eq!(b.spent_today_cu, 0, "spend counter must reset at 24h");
    }

    #[test]
    fn sec_deep_budget_no_rollover_just_before_24h() {
        // At 24h - 1ms, rollover must NOT trigger.
        let mut b = TrmBudget::default();
        let start = b.day_started_at_ms;
        b.record_spend(500).unwrap();

        let just_before = start + 24 * 3_600_000 - 1;
        let reset = b.maybe_reset_day(just_before);
        assert!(!reset, "rollover must NOT trigger at 24h - 1ms");
        assert_eq!(b.spent_today_cu, 500, "spend counter must remain unchanged before rollover");
    }

    #[test]
    fn sec_deep_budget_u64_max_timestamp_does_not_overflow() {
        // now_ms = u64::MAX must not overflow in saturating_sub with day_started_at_ms = 0.
        let mut b = TrmBudget {
            day_started_at_ms: 0,
            ..TrmBudget::default()
        };
        // u64::MAX - 0 = u64::MAX, which is >= 24h → rollover triggers.
        let result = std::panic::catch_unwind(move || {
            b.maybe_reset_day(u64::MAX)
        });
        assert!(result.is_ok(), "u64::MAX timestamp must not cause panic in maybe_reset_day");
    }

    #[test]
    fn sec_deep_budget_now_ms_before_day_start_does_not_rollover() {
        // now_ms < day_started_at_ms — saturating_sub produces 0 → no rollover.
        let mut b = TrmBudget {
            day_started_at_ms: 1_000_000,
            ..TrmBudget::default()
        };
        b.record_spend(100).unwrap();
        let reset = b.maybe_reset_day(500_000); // past → underflow saturates to 0
        assert!(!reset, "timestamp before day_start must not trigger rollover");
        assert_eq!(b.spent_today_cu, 100, "spend counter must be unchanged");
    }

    #[test]
    fn sec_deep_improvement_negative_delta_never_accepted() {
        let b = TrmBudget {
            min_score_delta: 0.01,
            min_roi_threshold: 0.0,
            ..TrmBudget::default()
        };
        // Negative delta must always be rejected regardless of ROI.
        assert!(
            !b.is_improvement_worth_keeping(-0.01, 0, 0),
            "negative delta must always produce Revert decision"
        );
        assert!(
            !b.is_improvement_worth_keeping(-100.0, 0, 1_000_000),
            "large negative delta with infinite ROI must still be rejected"
        );
    }

    #[test]
    fn sec_deep_improvement_zero_cost_positive_delta_accepted() {
        let b = TrmBudget {
            min_score_delta: 0.01,
            min_roi_threshold: 1.0, // ROI must be >= 1x
            ..TrmBudget::default()
        };
        // trm_invested = 0 → free improvement → cu_return_estimate / 0 is guarded.
        // The implementation: if trm_invested == 0, return score_delta > 0.
        assert!(
            b.is_improvement_worth_keeping(0.05, 0, 0),
            "zero-cost positive delta must always be accepted (free improvement)"
        );
    }

    #[test]
    fn sec_deep_improvement_tiny_delta_below_threshold_rejected() {
        let b = TrmBudget {
            min_score_delta: 0.01,
            min_roi_threshold: 0.0,
            ..TrmBudget::default()
        };
        // delta = 0.009 < min_score_delta 0.01 → rejected.
        assert!(
            !b.is_improvement_worth_keeping(0.009, 100, 1_000),
            "delta 0.009 below min_score_delta 0.01 must be rejected"
        );
        // delta = 0.01 (exactly at threshold) → accepted.
        assert!(
            b.is_improvement_worth_keeping(0.01, 100, 1_000),
            "delta exactly at min_score_delta threshold must be accepted"
        );
    }

    #[test]
    fn sec_deep_record_spend_exact_at_daily_limit() {
        let mut b = TrmBudget {
            max_trm_per_cycle: 50_000,
            max_trm_per_day: 50_000,
            max_cycles_per_day: 20,
            ..TrmBudget::default()
        };
        // Spend exactly the daily limit.
        b.record_spend(50_000).unwrap();
        assert_eq!(b.spent_today_cu, 50_000);
        // One more CU must be rejected.
        assert!(!b.can_spend(1), "one CU over daily limit must be rejected");
    }
}
