//! Phase 12 Research: Federated training scaffold
//!
//! Counterpart to `tirami_mind::cycle::ImprovementCycleRunner` — where the
//! cycle runner improves a SINGLE agent's harness via an optimizer, this
//! module implements DISTRIBUTED training where many nodes contribute
//! gradients to a shared model and are rewarded in CU proportional to how
//! much their gradient reduces the loss per unit of compute spent.
//!
//! ## Intended flow
//!
//! 1. A coordinator publishes a new `FederatedRound` with a base model hash
//!    and target sample count.
//! 2. Participating nodes pull the base model, compute gradients on their
//!    local data, and `submit` `GradientContribution` records.
//! 3. When `round.is_ready_to_aggregate()`, any node can call `aggregate`
//!    with the weighted-average aggregator and receive the list of
//!    rewards to pay out.
//! 4. Rewards are paid via the existing `ComputeLedger::execute_trade`
//!    path (provider = coordinator address, consumer = contributor).
//!
//! ## What's scaffold vs real
//!
//! Scaffold (this module): round management, contribution validation,
//! efficiency-weighted reward distribution, deterministic aggregated-model
//! hash. These are all ready for production use.
//!
//! NOT scaffold (Phase 13+): actually computing gradients, applying them
//! to model weights, broadcasting new weights to participants, gradient
//! compression, secure aggregation (differential privacy, homomorphic
//! encryption). Those are research topics that need a real training
//! backend (Candle, Burn, tch-rs) to plug into.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tirami_core::NodeId;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq)]
pub enum FederatedError {
    #[error("contribution round_id mismatch")]
    WrongRound,
    #[error("contribution base_model_hash mismatch")]
    WrongBaseModel,
    #[error("round already finalized")]
    RoundFinalized,
    #[error("duplicate contribution from same node")]
    DuplicateContribution,
    #[error("no contributions to aggregate")]
    NoContributions,
}

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A gradient contribution from one node in a federated training round.
/// Nodes compute gradients locally on their own data and submit them to a
/// coordinator; the coordinator aggregates weighted by reputation and mints CU
/// rewards for each contributor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GradientContribution {
    pub contributor: NodeId,
    /// Training round this contribution belongs to.
    pub round_id: u64,
    /// SHA-256 of the base model weights this gradient was computed against.
    pub base_model_hash: [u8; 32],
    /// SHA-256 of the gradient tensor itself.
    pub gradient_hash: [u8; 32],
    /// Number of training samples used.
    pub sample_count: u64,
    /// Loss value before this gradient (higher = more room to improve).
    pub loss_before: f64,
    /// Loss value after applying this gradient (lower = better).
    pub loss_after: f64,
    /// Compute spent (in CU) to produce this gradient.
    pub compute_cost_trm: u64,
    /// Timestamp of the gradient computation.
    pub timestamp_ms: u64,
}

impl GradientContribution {
    /// Loss improvement. Positive = gradient reduced loss.
    pub fn loss_delta(&self) -> f64 {
        self.loss_before - self.loss_after
    }

    /// Efficiency score: loss improvement per CU spent.
    /// Used as the aggregation weight.
    pub fn efficiency(&self) -> f64 {
        if self.compute_cost_trm == 0 {
            0.0
        } else {
            self.loss_delta() / self.compute_cost_trm as f64
        }
    }
}

/// A federated training round: base model + collection of contributions +
/// aggregation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedRound {
    pub round_id: u64,
    pub base_model_hash: [u8; 32],
    pub contributions: Vec<GradientContribution>,
    pub target_sample_count: u64,
    /// Set after aggregate() runs.
    pub aggregated_hash: Option<[u8; 32]>,
    /// Total CU rewards distributed across contributors.
    pub total_rewards_cu: u64,
    pub started_at_ms: u64,
    pub finalized_at_ms: Option<u64>,
}

impl FederatedRound {
    pub fn new(
        round_id: u64,
        base_model_hash: [u8; 32],
        target_sample_count: u64,
        now_ms: u64,
    ) -> Self {
        Self {
            round_id,
            base_model_hash,
            contributions: Vec::new(),
            target_sample_count,
            aggregated_hash: None,
            total_rewards_cu: 0,
            started_at_ms: now_ms,
            finalized_at_ms: None,
        }
    }

    pub fn submit(&mut self, contribution: GradientContribution) -> Result<(), FederatedError> {
        if contribution.round_id != self.round_id {
            return Err(FederatedError::WrongRound);
        }
        if contribution.base_model_hash != self.base_model_hash {
            return Err(FederatedError::WrongBaseModel);
        }
        if self.finalized_at_ms.is_some() {
            return Err(FederatedError::RoundFinalized);
        }
        // Reject duplicates from the same contributor
        if self
            .contributions
            .iter()
            .any(|c| c.contributor == contribution.contributor)
        {
            return Err(FederatedError::DuplicateContribution);
        }
        self.contributions.push(contribution);
        Ok(())
    }

    pub fn sample_coverage(&self) -> u64 {
        self.contributions.iter().map(|c| c.sample_count).sum()
    }

    pub fn is_ready_to_aggregate(&self) -> bool {
        self.sample_coverage() >= self.target_sample_count
    }
}

// ---------------------------------------------------------------------------
// Aggregator trait + result type
// ---------------------------------------------------------------------------

pub trait Aggregator: Send + Sync {
    fn name(&self) -> &str;

    /// Aggregate contributions into a new model hash + distribute rewards.
    /// Returns (new_aggregated_hash, rewards_per_contributor).
    fn aggregate(
        &self,
        round: &FederatedRound,
        reward_pool_cu: u64,
    ) -> Result<AggregationResult, FederatedError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregationResult {
    pub aggregated_hash: [u8; 32],
    /// (contributor, reward_cu)
    pub rewards: Vec<(NodeId, u64)>,
}

// ---------------------------------------------------------------------------
// WeightedAverageAggregator
// ---------------------------------------------------------------------------

/// Simple aggregator: weight each contribution by efficiency() (loss delta per
/// CU spent) and distribute reward_pool_cu proportionally.
pub struct WeightedAverageAggregator;

impl Aggregator for WeightedAverageAggregator {
    fn name(&self) -> &str {
        "weighted-average"
    }

    fn aggregate(
        &self,
        round: &FederatedRound,
        reward_pool_cu: u64,
    ) -> Result<AggregationResult, FederatedError> {
        if round.contributions.is_empty() {
            return Err(FederatedError::NoContributions);
        }

        // Aggregated hash = SHA-256 of round_id + base_hash + all gradient_hashes sorted
        // by contributor id for determinism.
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(round.round_id.to_be_bytes());
        hasher.update(round.base_model_hash);
        let mut sorted: Vec<&GradientContribution> = round.contributions.iter().collect();
        sorted.sort_by(|a, b| a.contributor.0.cmp(&b.contributor.0));
        for c in &sorted {
            hasher.update(c.gradient_hash);
        }
        let aggregated_bytes = hasher.finalize();
        let mut aggregated_hash = [0u8; 32];
        aggregated_hash.copy_from_slice(&aggregated_bytes);

        // Distribute rewards by efficiency (clamped to >= 0 so negative gradients
        // don't steal from the pool).
        let total_efficiency: f64 = sorted.iter().map(|c| c.efficiency().max(0.0)).sum();
        let mut rewards: Vec<(NodeId, u64)> = Vec::new();

        if total_efficiency > 0.0 {
            for c in &sorted {
                let share = c.efficiency().max(0.0) / total_efficiency;
                let reward = (reward_pool_cu as f64 * share) as u64;
                if reward > 0 {
                    rewards.push((c.contributor.clone(), reward));
                }
            }
        } else {
            // Fallback: equal split among all contributors when no one improves loss.
            let per = reward_pool_cu / sorted.len() as u64;
            for c in &sorted {
                rewards.push((c.contributor.clone(), per));
            }
        }

        Ok(AggregationResult {
            aggregated_hash,
            rewards,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::NodeId;

    fn make_node(seed: u8) -> NodeId {
        NodeId([seed; 32])
    }

    fn make_contribution(
        seed: u8,
        round_id: u64,
        samples: u64,
        loss_before: f64,
        loss_after: f64,
        cu: u64,
    ) -> GradientContribution {
        GradientContribution {
            contributor: make_node(seed),
            round_id,
            base_model_hash: [1u8; 32],
            gradient_hash: [seed; 32],
            sample_count: samples,
            loss_before,
            loss_after,
            compute_cost_trm: cu,
            timestamp_ms: 1_700_000_000_000,
        }
    }

    // --- GradientContribution helpers ---

    #[test]
    fn test_loss_delta() {
        let c = make_contribution(1, 0, 100, 2.0, 1.5, 50);
        assert!((c.loss_delta() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_loss_delta_negative_when_loss_increases() {
        let c = make_contribution(1, 0, 100, 1.0, 1.5, 50);
        assert!(c.loss_delta() < 0.0);
    }

    #[test]
    fn test_efficiency_zero_when_cost_zero() {
        let c = make_contribution(1, 0, 100, 2.0, 1.0, 0);
        assert_eq!(c.efficiency(), 0.0);
    }

    #[test]
    fn test_efficiency_computed_correctly() {
        // loss_delta = 0.5, cost = 100 → efficiency = 0.005
        let c = make_contribution(1, 0, 100, 1.0, 0.5, 100);
        assert!((c.efficiency() - 0.005).abs() < 1e-10);
    }

    // --- FederatedRound::submit ---

    #[test]
    fn test_round_submit_accepts_matching_round() {
        let mut round = FederatedRound::new(1, [1u8; 32], 100, 0);
        let c = make_contribution(1, 1, 50, 1.0, 0.8, 10);
        assert!(round.submit(c).is_ok());
        assert_eq!(round.contributions.len(), 1);
    }

    #[test]
    fn test_round_submit_rejects_wrong_round_id() {
        let mut round = FederatedRound::new(1, [1u8; 32], 100, 0);
        let c = make_contribution(1, 99, 50, 1.0, 0.8, 10);
        assert_eq!(round.submit(c), Err(FederatedError::WrongRound));
    }

    #[test]
    fn test_round_submit_rejects_wrong_base_model() {
        let mut round = FederatedRound::new(1, [1u8; 32], 100, 0);
        let mut c = make_contribution(1, 1, 50, 1.0, 0.8, 10);
        c.base_model_hash = [2u8; 32]; // wrong hash
        assert_eq!(round.submit(c), Err(FederatedError::WrongBaseModel));
    }

    #[test]
    fn test_round_submit_rejects_duplicate_contributor() {
        let mut round = FederatedRound::new(1, [1u8; 32], 100, 0);
        let c1 = make_contribution(1, 1, 50, 1.0, 0.8, 10);
        let c2 = make_contribution(1, 1, 50, 1.0, 0.7, 10); // same node seed=1
        round.submit(c1).unwrap();
        assert_eq!(round.submit(c2), Err(FederatedError::DuplicateContribution));
    }

    #[test]
    fn test_round_submit_rejects_after_finalized() {
        let mut round = FederatedRound::new(1, [1u8; 32], 100, 0);
        round.finalized_at_ms = Some(999);
        let c = make_contribution(1, 1, 50, 1.0, 0.8, 10);
        assert_eq!(round.submit(c), Err(FederatedError::RoundFinalized));
    }

    // --- FederatedRound helpers ---

    #[test]
    fn test_sample_coverage_sums() {
        let mut round = FederatedRound::new(1, [1u8; 32], 200, 0);
        round.submit(make_contribution(1, 1, 80, 1.0, 0.9, 10)).unwrap();
        round.submit(make_contribution(2, 1, 70, 1.0, 0.9, 10)).unwrap();
        assert_eq!(round.sample_coverage(), 150);
    }

    #[test]
    fn test_is_ready_to_aggregate() {
        let mut round = FederatedRound::new(1, [1u8; 32], 100, 0);
        assert!(!round.is_ready_to_aggregate());
        round.submit(make_contribution(1, 1, 60, 1.0, 0.9, 10)).unwrap();
        assert!(!round.is_ready_to_aggregate());
        round.submit(make_contribution(2, 1, 50, 1.0, 0.9, 10)).unwrap();
        assert!(round.is_ready_to_aggregate());
    }

    // --- WeightedAverageAggregator ---

    #[test]
    fn test_aggregate_empty_fails() {
        let round = FederatedRound::new(1, [1u8; 32], 100, 0);
        let agg = WeightedAverageAggregator;
        assert_eq!(agg.aggregate(&round, 1000), Err(FederatedError::NoContributions));
    }

    #[test]
    fn test_aggregate_distributes_rewards_by_efficiency() {
        // contributor A (seed=1): loss 1.0 → 0.5, cost 100 → efficiency 0.005
        // contributor B (seed=2): loss 1.0 → 0.8, cost 100 → efficiency 0.002
        // total efficiency = 0.007
        // A share = 0.005/0.007 ≈ 0.714 → reward ≈ 714
        // B share = 0.002/0.007 ≈ 0.286 → reward ≈ 285
        let mut round = FederatedRound::new(1, [1u8; 32], 10, 0);
        round.submit(make_contribution(1, 1, 5, 1.0, 0.5, 100)).unwrap();
        round.submit(make_contribution(2, 1, 5, 1.0, 0.8, 100)).unwrap();

        let agg = WeightedAverageAggregator;
        let result = agg.aggregate(&round, 1000).unwrap();

        // Find rewards for each node
        let reward_a = result.rewards.iter().find(|(n, _)| *n == make_node(1)).map(|(_, r)| *r).unwrap_or(0);
        let reward_b = result.rewards.iter().find(|(n, _)| *n == make_node(2)).map(|(_, r)| *r).unwrap_or(0);

        // A should get significantly more than B
        assert!(reward_a > reward_b, "A ({reward_a}) should outreward B ({reward_b})");
        // Sanity: A ~714, B ~285 (integer division loses a few CU)
        assert!(reward_a >= 700 && reward_a <= 720, "reward_a={reward_a}");
        assert!(reward_b >= 280 && reward_b <= 290, "reward_b={reward_b}");
        // Total rewards <= pool (integer truncation means sum may be slightly less)
        assert!(reward_a + reward_b <= 1000);
    }

    #[test]
    fn test_aggregate_equal_split_when_no_efficiency() {
        // Both contributors have loss_after == loss_before → efficiency 0
        // Pool = 1000, 2 contributors → each gets 500
        let mut round = FederatedRound::new(1, [1u8; 32], 10, 0);
        round.submit(make_contribution(1, 1, 5, 1.0, 1.0, 100)).unwrap();
        round.submit(make_contribution(2, 1, 5, 1.0, 1.0, 100)).unwrap();

        let agg = WeightedAverageAggregator;
        let result = agg.aggregate(&round, 1000).unwrap();

        let r1 = result.rewards.iter().find(|(n, _)| *n == make_node(1)).map(|(_, r)| *r).unwrap_or(0);
        let r2 = result.rewards.iter().find(|(n, _)| *n == make_node(2)).map(|(_, r)| *r).unwrap_or(0);
        assert_eq!(r1, 500);
        assert_eq!(r2, 500);
    }

    #[test]
    fn test_aggregate_deterministic_hash() {
        // Two identical rounds produce identical aggregated_hash
        let mut round_a = FederatedRound::new(1, [1u8; 32], 10, 0);
        round_a.submit(make_contribution(1, 1, 5, 1.0, 0.8, 10)).unwrap();
        round_a.submit(make_contribution(2, 1, 5, 1.0, 0.7, 10)).unwrap();

        let mut round_b = FederatedRound::new(1, [1u8; 32], 10, 0);
        // Submit in reverse order — sorting should make hash identical
        round_b.submit(make_contribution(2, 1, 5, 1.0, 0.7, 10)).unwrap();
        round_b.submit(make_contribution(1, 1, 5, 1.0, 0.8, 10)).unwrap();

        let agg = WeightedAverageAggregator;
        let r_a = agg.aggregate(&round_a, 1000).unwrap();
        let r_b = agg.aggregate(&round_b, 1000).unwrap();
        assert_eq!(r_a.aggregated_hash, r_b.aggregated_hash);
    }

    #[test]
    fn test_aggregate_negative_efficiency_contributors_get_no_reward() {
        // Contributor A improves loss; contributor B makes it worse (negative efficiency).
        // B should get 0 reward (negative efficiency is clamped to 0).
        let mut round = FederatedRound::new(1, [1u8; 32], 10, 0);
        round.submit(make_contribution(1, 1, 5, 1.0, 0.5, 100)).unwrap(); // good
        round.submit(make_contribution(2, 1, 5, 1.0, 1.5, 100)).unwrap(); // bad

        let agg = WeightedAverageAggregator;
        let result = agg.aggregate(&round, 1000).unwrap();

        let reward_b = result.rewards.iter().find(|(n, _)| *n == make_node(2)).map(|(_, r)| *r).unwrap_or(0);
        assert_eq!(reward_b, 0, "bad contributor should get no reward");
    }
}
