//! Staking system: lock TRM for reputation multiplier + graduated slashing.

use serde::{Deserialize, Serialize};
use tirami_core::NodeId;
use std::collections::HashMap;

// Constants from parameters.md §16
pub const STAKING_7D_MIN: u64 = 100;
pub const STAKING_7D_MULTIPLIER: f64 = 1.2;
pub const STAKING_30D_MIN: u64 = 1_000;
pub const STAKING_30D_MULTIPLIER: f64 = 1.5;
pub const STAKING_90D_MIN: u64 = 10_000;
pub const STAKING_90D_MULTIPLIER: f64 = 2.0;
pub const STAKING_365D_MIN: u64 = 100_000;
pub const STAKING_365D_MULTIPLIER: f64 = 3.0;

pub const SLASH_RATE_MINOR: f64 = 0.05;    // trust_penalty 0.1-0.2
pub const SLASH_RATE_MAJOR: f64 = 0.20;    // trust_penalty 0.2-0.4
pub const SLASH_RATE_CRITICAL: f64 = 0.50; // trust_penalty 0.4-0.5

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakeDuration {
    Days7,
    Days30,
    Days90,
    Days365,
}

impl StakeDuration {
    pub fn min_amount(&self) -> u64 {
        match self {
            Self::Days7 => STAKING_7D_MIN,
            Self::Days30 => STAKING_30D_MIN,
            Self::Days90 => STAKING_90D_MIN,
            Self::Days365 => STAKING_365D_MIN,
        }
    }

    pub fn multiplier(&self) -> f64 {
        match self {
            Self::Days7 => STAKING_7D_MULTIPLIER,
            Self::Days30 => STAKING_30D_MULTIPLIER,
            Self::Days90 => STAKING_90D_MULTIPLIER,
            Self::Days365 => STAKING_365D_MULTIPLIER,
        }
    }

    pub fn duration_ms(&self) -> u64 {
        match self {
            Self::Days7 => 7 * 24 * 3_600_000,
            Self::Days30 => 30 * 24 * 3_600_000,
            Self::Days90 => 90 * 24 * 3_600_000,
            Self::Days365 => 365 * 24 * 3_600_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stake {
    pub node_id: NodeId,
    pub amount: u64,
    pub duration: StakeDuration,
    pub locked_at_ms: u64,
    pub unlocks_at_ms: u64,
}

impl Stake {
    pub fn new(
        node_id: NodeId,
        amount: u64,
        duration: StakeDuration,
        now_ms: u64,
    ) -> Result<Self, StakingError> {
        if amount < duration.min_amount() {
            return Err(StakingError::InsufficientAmount {
                provided: amount,
                minimum: duration.min_amount(),
            });
        }
        Ok(Self {
            node_id,
            amount,
            duration,
            locked_at_ms: now_ms,
            unlocks_at_ms: now_ms + duration.duration_ms(),
        })
    }

    pub fn is_locked(&self, now_ms: u64) -> bool {
        now_ms < self.unlocks_at_ms
    }

    pub fn multiplier(&self) -> f64 {
        self.duration.multiplier()
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum StakingError {
    #[error("insufficient stake: {provided} < minimum {minimum}")]
    InsufficientAmount { provided: u64, minimum: u64 },
    #[error("insufficient balance to stake")]
    InsufficientBalance,
    #[error("node already has an active stake")]
    AlreadyStaked,
    #[error("stake is still locked")]
    StillLocked,
}

/// Compute slash rate from trust_penalty.
pub fn slash_rate(trust_penalty: f64) -> f64 {
    if trust_penalty < 0.1 {
        0.0
    } else if trust_penalty < 0.2 {
        SLASH_RATE_MINOR
    } else if trust_penalty < 0.4 {
        SLASH_RATE_MAJOR
    } else {
        SLASH_RATE_CRITICAL
    }
}

/// Compute the amount to slash from a stake given a trust_penalty.
/// Returns the TRM to burn (removed from circulation permanently).
pub fn compute_slash(staked_amount: u64, trust_penalty: f64) -> u64 {
    let rate = slash_rate(trust_penalty);
    (staked_amount as f64 * rate) as u64
}

/// Manages all stakes for the network.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StakingPool {
    pub stakes: HashMap<NodeId, Stake>,
    /// Total TRM burned via slashing (deflationary).
    pub total_burned: u64,
}

impl StakingPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new stake for a node. Returns error if already staked or insufficient amount.
    pub fn stake(
        &mut self,
        node_id: NodeId,
        amount: u64,
        duration: StakeDuration,
        now_ms: u64,
    ) -> Result<&Stake, StakingError> {
        if self.stakes.contains_key(&node_id) {
            if self.stakes[&node_id].is_locked(now_ms) {
                return Err(StakingError::AlreadyStaked);
            }
            // Previous stake expired — allow re-staking
            self.stakes.remove(&node_id);
        }
        let stake = Stake::new(node_id.clone(), amount, duration, now_ms)?;
        self.stakes.insert(node_id.clone(), stake);
        Ok(&self.stakes[&node_id])
    }

    /// Unstake (withdraw) after lock period expires.
    pub fn unstake(&mut self, node_id: &NodeId, now_ms: u64) -> Result<u64, StakingError> {
        let stake = self
            .stakes
            .get(node_id)
            .ok_or(StakingError::InsufficientBalance)?;
        if stake.is_locked(now_ms) {
            return Err(StakingError::StillLocked);
        }
        let amount = stake.amount;
        self.stakes.remove(node_id);
        Ok(amount)
    }

    /// Get the staking multiplier for a node. Returns 1.0 if not staked.
    pub fn multiplier(&self, node_id: &NodeId, now_ms: u64) -> f64 {
        self.stakes
            .get(node_id)
            .filter(|s| s.is_locked(now_ms))
            .map(|s| s.multiplier())
            .unwrap_or(1.0)
    }

    /// Apply slashing to a node based on trust_penalty from collusion detection.
    /// Returns the amount burned. The burned TRM is permanently removed.
    pub fn apply_slash(&mut self, node_id: &NodeId, trust_penalty: f64) -> u64 {
        let Some(stake) = self.stakes.get_mut(node_id) else {
            return 0;
        };
        let burn = compute_slash(stake.amount, trust_penalty);
        if burn > 0 {
            stake.amount = stake.amount.saturating_sub(burn);
            self.total_burned += burn;
        }
        burn
    }

    /// Total TRM currently locked in stakes.
    pub fn total_staked(&self) -> u64 {
        self.stakes.values().map(|s| s.amount).sum()
    }

    /// Phase 18.2 — Query every `Stake` record belonging to `node_id`.
    /// Currently the map stores at most one stake per node, but the
    /// return shape is iterator-typed so future designs with multiple
    /// concurrent stakes (e.g. different durations) don't break callers.
    pub fn stakes_for(&self, node_id: &NodeId) -> impl Iterator<Item = &Stake> {
        self.stakes
            .get(node_id)
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::NodeId;

    fn node(seed: u8) -> NodeId {
        NodeId([seed; 32])
    }

    const NOW: u64 = 1_000_000_000; // arbitrary reference timestamp in ms

    // --- StakeDuration constants ---

    #[test]
    fn test_multiplier_7d() {
        assert_eq!(StakeDuration::Days7.multiplier(), 1.2);
    }

    #[test]
    fn test_multiplier_30d() {
        assert_eq!(StakeDuration::Days30.multiplier(), 1.5);
    }

    #[test]
    fn test_multiplier_90d() {
        assert_eq!(StakeDuration::Days90.multiplier(), 2.0);
    }

    #[test]
    fn test_multiplier_365d() {
        assert_eq!(StakeDuration::Days365.multiplier(), 3.0);
    }

    #[test]
    fn test_min_amounts() {
        assert_eq!(StakeDuration::Days7.min_amount(), STAKING_7D_MIN);
        assert_eq!(StakeDuration::Days30.min_amount(), STAKING_30D_MIN);
        assert_eq!(StakeDuration::Days90.min_amount(), STAKING_90D_MIN);
        assert_eq!(StakeDuration::Days365.min_amount(), STAKING_365D_MIN);
    }

    #[test]
    fn test_duration_ms_7d() {
        assert_eq!(StakeDuration::Days7.duration_ms(), 7 * 24 * 3_600_000);
    }

    // --- Stake::new ---

    #[test]
    fn test_stake_creation_7d() {
        let s = Stake::new(node(1), 200, StakeDuration::Days7, NOW).unwrap();
        assert_eq!(s.amount, 200);
        assert_eq!(s.duration, StakeDuration::Days7);
        assert_eq!(s.locked_at_ms, NOW);
        assert_eq!(s.unlocks_at_ms, NOW + StakeDuration::Days7.duration_ms());
    }

    #[test]
    fn test_stake_rejects_below_minimum() {
        let err = Stake::new(node(2), 50, StakeDuration::Days7, NOW).unwrap_err();
        assert_eq!(
            err,
            StakingError::InsufficientAmount {
                provided: 50,
                minimum: STAKING_7D_MIN,
            }
        );
    }

    #[test]
    fn test_stake_rejects_below_minimum_30d() {
        let err = Stake::new(node(3), 500, StakeDuration::Days30, NOW).unwrap_err();
        assert_eq!(
            err,
            StakingError::InsufficientAmount {
                provided: 500,
                minimum: STAKING_30D_MIN,
            }
        );
    }

    #[test]
    fn test_stake_is_locked_during_period() {
        let s = Stake::new(node(4), 1_000, StakeDuration::Days7, NOW).unwrap();
        // 1 ms before unlock
        assert!(s.is_locked(NOW + StakeDuration::Days7.duration_ms() - 1));
    }

    #[test]
    fn test_stake_unlocks_after_period() {
        let s = Stake::new(node(5), 1_000, StakeDuration::Days7, NOW).unwrap();
        assert!(!s.is_locked(NOW + StakeDuration::Days7.duration_ms()));
    }

    #[test]
    fn test_stake_multiplier_matches_duration() {
        let s = Stake::new(node(6), 10_000, StakeDuration::Days90, NOW).unwrap();
        assert_eq!(s.multiplier(), 2.0);
    }

    // --- slash_rate ---

    #[test]
    fn test_slash_rate_below_threshold() {
        assert_eq!(slash_rate(0.05), 0.0);
    }

    #[test]
    fn test_slash_rate_at_zero() {
        assert_eq!(slash_rate(0.0), 0.0);
    }

    #[test]
    fn test_slash_rate_minor() {
        assert_eq!(slash_rate(0.15), SLASH_RATE_MINOR);
    }

    #[test]
    fn test_slash_rate_major() {
        assert_eq!(slash_rate(0.3), SLASH_RATE_MAJOR);
    }

    #[test]
    fn test_slash_rate_critical() {
        assert_eq!(slash_rate(0.45), SLASH_RATE_CRITICAL);
    }

    #[test]
    fn test_slash_rate_exactly_at_boundaries() {
        // 0.1 is the start of minor range
        assert_eq!(slash_rate(0.1), SLASH_RATE_MINOR);
        // 0.2 is the start of major range
        assert_eq!(slash_rate(0.2), SLASH_RATE_MAJOR);
        // 0.4 is the start of critical range
        assert_eq!(slash_rate(0.4), SLASH_RATE_CRITICAL);
    }

    // --- compute_slash ---

    #[test]
    fn test_compute_slash_minor() {
        // 5% of 10_000 = 500
        assert_eq!(compute_slash(10_000, 0.15), 500);
    }

    #[test]
    fn test_compute_slash_critical() {
        // 50% of 10_000 = 5_000
        assert_eq!(compute_slash(10_000, 0.45), 5_000);
    }

    #[test]
    fn test_compute_slash_no_penalty() {
        assert_eq!(compute_slash(10_000, 0.05), 0);
    }

    // --- StakingPool ---

    #[test]
    fn test_pool_stake_and_multiplier() {
        let mut pool = StakingPool::new();
        pool.stake(node(10), 1_000, StakeDuration::Days30, NOW).unwrap();
        assert_eq!(pool.multiplier(&node(10), NOW + 1), 1.5);
    }

    #[test]
    fn test_multiplier_returns_1_when_not_staked() {
        let pool = StakingPool::new();
        assert_eq!(pool.multiplier(&node(20), NOW), 1.0);
    }

    #[test]
    fn test_multiplier_returns_1_after_expiry() {
        let mut pool = StakingPool::new();
        pool.stake(node(21), 1_000, StakeDuration::Days7, NOW).unwrap();
        let after = NOW + StakeDuration::Days7.duration_ms();
        // Expired stake → no multiplier benefit
        assert_eq!(pool.multiplier(&node(21), after), 1.0);
    }

    #[test]
    fn test_cannot_double_stake() {
        let mut pool = StakingPool::new();
        pool.stake(node(30), 1_000, StakeDuration::Days30, NOW).unwrap();
        let err = pool
            .stake(node(30), 2_000, StakeDuration::Days30, NOW + 1)
            .unwrap_err();
        assert_eq!(err, StakingError::AlreadyStaked);
    }

    #[test]
    fn test_can_restake_after_expiry() {
        let mut pool = StakingPool::new();
        pool.stake(node(31), 1_000, StakeDuration::Days7, NOW).unwrap();
        let after = NOW + StakeDuration::Days7.duration_ms();
        // Should succeed — old stake expired
        pool.stake(node(31), 2_000, StakeDuration::Days30, after).unwrap();
        assert_eq!(pool.multiplier(&node(31), after + 1), 1.5);
    }

    #[test]
    fn test_unstake_before_expiry_fails() {
        let mut pool = StakingPool::new();
        pool.stake(node(40), 1_000, StakeDuration::Days30, NOW).unwrap();
        let err = pool.unstake(&node(40), NOW + 1).unwrap_err();
        assert_eq!(err, StakingError::StillLocked);
    }

    #[test]
    fn test_unstake_after_expiry_succeeds() {
        let mut pool = StakingPool::new();
        pool.stake(node(41), 5_000, StakeDuration::Days30, NOW).unwrap();
        let after = NOW + StakeDuration::Days30.duration_ms();
        let returned = pool.unstake(&node(41), after).unwrap();
        assert_eq!(returned, 5_000);
        // Gone from pool
        assert!(pool.stakes.get(&node(41)).is_none());
    }

    #[test]
    fn test_apply_slash_burns_correct_amount() {
        let mut pool = StakingPool::new();
        pool.stake(node(50), 10_000, StakeDuration::Days90, NOW).unwrap();
        let burned = pool.apply_slash(&node(50), 0.45); // critical → 50%
        assert_eq!(burned, 5_000);
    }

    #[test]
    fn test_slash_reduces_stake_amount() {
        let mut pool = StakingPool::new();
        pool.stake(node(51), 10_000, StakeDuration::Days90, NOW).unwrap();
        pool.apply_slash(&node(51), 0.3); // major → 20%
        assert_eq!(pool.stakes[&node(51)].amount, 8_000);
    }

    #[test]
    fn test_total_burned_accumulates() {
        let mut pool = StakingPool::new();
        pool.stake(node(60), 10_000, StakeDuration::Days90, NOW).unwrap();
        pool.stake(node(61), 10_000, StakeDuration::Days90, NOW).unwrap();
        pool.apply_slash(&node(60), 0.15); // minor → 5% = 500
        pool.apply_slash(&node(61), 0.3);  // major → 20% = 2000
        assert_eq!(pool.total_burned, 2_500);
    }

    #[test]
    fn test_slash_no_stake_returns_zero() {
        let mut pool = StakingPool::new();
        let burned = pool.apply_slash(&node(70), 0.5);
        assert_eq!(burned, 0);
        assert_eq!(pool.total_burned, 0);
    }

    #[test]
    fn test_pool_total_staked() {
        let mut pool = StakingPool::new();
        pool.stake(node(80), 1_000, StakeDuration::Days30, NOW).unwrap();
        pool.stake(node(81), 10_000, StakeDuration::Days90, NOW).unwrap();
        assert_eq!(pool.total_staked(), 11_000);
    }

    #[test]
    fn test_total_staked_after_slash() {
        let mut pool = StakingPool::new();
        pool.stake(node(90), 10_000, StakeDuration::Days90, NOW).unwrap();
        pool.apply_slash(&node(90), 0.3); // burns 2_000
        assert_eq!(pool.total_staked(), 8_000);
    }

    #[test]
    fn test_unstake_nonexistent_node() {
        let mut pool = StakingPool::new();
        let err = pool.unstake(&node(99), NOW).unwrap_err();
        assert_eq!(err, StakingError::InsufficientBalance);
    }

    #[test]
    fn test_unstake_removes_from_total() {
        let mut pool = StakingPool::new();
        pool.stake(node(100), 1_000, StakeDuration::Days7, NOW).unwrap();
        pool.stake(node(101), 2_000, StakeDuration::Days7, NOW).unwrap();
        let after = NOW + StakeDuration::Days7.duration_ms();
        pool.unstake(&node(100), after).unwrap();
        assert_eq!(pool.total_staked(), 2_000);
    }
}
