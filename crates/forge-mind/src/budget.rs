//! CU budget management for self-improvement.
//!
//! A `CuBudget` is a hard cap on how much the self-improvement loop can spend.
//! It tracks per-cycle and per-day spending, and gates each spend with a
//! predicate.

use serde::{Deserialize, Serialize};

/// Spending policy for the forge-mind self-improvement loop.
///
/// All limits are HARD limits — the loop will not spend even one CU above
/// the policy, even if it would yield a great improvement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuBudget {
    // Per-cycle limits
    pub max_cu_per_cycle: u64,

    // Per-day limits
    pub max_cu_per_day: u64,
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

impl Default for CuBudget {
    fn default() -> Self {
        Self {
            max_cu_per_cycle: 5_000,
            max_cu_per_day: 50_000,
            max_cycles_per_day: 20,
            min_score_delta: 0.01,
            min_roi_threshold: 1.0,
            spent_today_cu: 0,
            cycles_today: 0,
            day_started_at_ms: 0,
        }
    }
}

impl CuBudget {
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
    pub fn can_spend(&self, cu_amount: u64) -> bool {
        if cu_amount == 0 {
            return false;
        }
        if cu_amount > self.max_cu_per_cycle {
            return false;
        }
        if self.spent_today_cu + cu_amount > self.max_cu_per_day {
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
    pub fn record_spend(&mut self, cu_amount: u64) -> Result<(), String> {
        if !self.can_spend(cu_amount) {
            return Err(format!(
                "refused to record spend of {} CU — exceeds budget",
                cu_amount
            ));
        }
        self.spent_today_cu += cu_amount;
        Ok(())
    }

    /// Decide whether an improvement should be kept based on policy.
    pub fn is_improvement_worth_keeping(
        &self,
        score_delta: f64,
        cu_invested: u64,
        cu_return_estimate: u64,
    ) -> bool {
        if score_delta < self.min_score_delta {
            return false;
        }
        if cu_invested == 0 {
            return score_delta > 0.0; // Free improvement, always keep
        }
        let roi = cu_return_estimate as f64 / cu_invested as f64;
        roi >= self.min_roi_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_budget_allows_typical_spends() {
        let b = CuBudget::default();
        assert!(b.can_spend(100));
        assert!(b.can_spend(b.max_cu_per_cycle));
    }

    #[test]
    fn test_can_spend_rejects_zero() {
        let b = CuBudget::default();
        assert!(!b.can_spend(0));
    }

    #[test]
    fn test_can_spend_rejects_above_per_cycle_cap() {
        let b = CuBudget {
            max_cu_per_cycle: 1_000,
            ..CuBudget::default()
        };
        assert!(b.can_spend(1_000));
        assert!(!b.can_spend(1_001));
    }

    #[test]
    fn test_can_spend_rejects_above_per_day_cap() {
        let mut b = CuBudget {
            max_cu_per_cycle: 10_000,
            max_cu_per_day: 10_000,
            ..CuBudget::default()
        };
        b.record_spend(8_000).unwrap();
        assert!(b.can_spend(2_000));
        assert!(!b.can_spend(2_001));
    }

    #[test]
    fn test_record_spend_increments_counter() {
        let mut b = CuBudget::default();
        b.record_spend(500).unwrap();
        assert_eq!(b.spent_today_cu, 500);
        b.record_spend(200).unwrap();
        assert_eq!(b.spent_today_cu, 700);
    }

    #[test]
    fn test_record_spend_refuses_when_over_budget() {
        let mut b = CuBudget {
            max_cu_per_cycle: 100,
            max_cu_per_day: 100,
            ..CuBudget::default()
        };
        b.record_spend(100).unwrap();
        assert!(b.record_spend(1).is_err());
    }

    #[test]
    fn test_cycle_count_limit() {
        let mut b = CuBudget {
            max_cycles_per_day: 3,
            ..CuBudget::default()
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
        let mut b = CuBudget {
            max_cycles_per_day: 2,
            ..CuBudget::default()
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
        let mut b = CuBudget::default();
        b.record_spend(500).unwrap();
        let later = b.day_started_at_ms + 3_600_000; // 1 hour
        let reset = b.maybe_reset_day(later);
        assert!(!reset);
        assert_eq!(b.spent_today_cu, 500);
    }

    #[test]
    fn test_is_improvement_worth_keeping_rejects_low_delta() {
        let b = CuBudget {
            min_score_delta: 0.05,
            ..CuBudget::default()
        };
        assert!(!b.is_improvement_worth_keeping(0.01, 100, 1000));
    }

    #[test]
    fn test_is_improvement_worth_keeping_accepts_good_roi() {
        let b = CuBudget {
            min_score_delta: 0.01,
            min_roi_threshold: 2.0,
            ..CuBudget::default()
        };
        assert!(b.is_improvement_worth_keeping(0.05, 100, 300));
    }

    #[test]
    fn test_is_improvement_worth_keeping_rejects_low_roi() {
        let b = CuBudget {
            min_score_delta: 0.01,
            min_roi_threshold: 2.0,
            ..CuBudget::default()
        };
        assert!(!b.is_improvement_worth_keeping(0.05, 100, 150));
    }

    #[test]
    fn test_zero_investment_improvement_always_kept_if_delta_positive() {
        let b = CuBudget::default();
        assert!(b.is_improvement_worth_keeping(0.1, 0, 0));
    }

    #[test]
    fn test_budget_rolls_over_after_24h() {
        let mut budget = CuBudget::default();
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
        let mut b = CuBudget {
            max_cycles_per_day: 1,
            ..CuBudget::default()
        };
        // With 0 cycles used, can_spend should work (cycles_today < max)
        assert!(b.can_spend(100));
        // Once we use the cycle slot, can_spend returns false
        b.record_cycle_start().unwrap();
        assert!(!b.can_spend(100));
    }
}
