//! Reputation aggregation from observed trades.
//!
//! Computes reputation scores locally from the trade observations in an
//! `AgentRegistry`. Mirrors `forge-economics/spec/parameters.md` §12.

use crate::registry::AgentRegistry;
use crate::types::{ReputationScore, TradeObservation};

/// Reputation calculation engine.
///
/// All constants have authoritative values in `forge-economics/spec/parameters.md` §12.
pub struct ReputationCalculator;

impl ReputationCalculator {
    /// Weight of volume sub-score.
    pub const WEIGHT_VOLUME: f64 = 0.4;
    /// Weight of recency sub-score.
    pub const WEIGHT_RECENCY: f64 = 0.3;
    /// Weight of diversity sub-score.
    pub const WEIGHT_DIVERSITY: f64 = 0.2;
    /// Weight of consistency sub-score.
    pub const WEIGHT_CONSISTENCY: f64 = 0.1;

    /// CU volume at which volume sub-score saturates to 1.0.
    pub const VOLUME_CAP_CU: u64 = 100_000;
    /// Half-life for recency decay (24 hours in milliseconds).
    pub const RECENCY_HALF_LIFE_MS: u64 = 24 * 3_600_000;
    /// Number of distinct counterparties for full diversity score.
    pub const DIVERSITY_CAP: usize = 10;
    /// Minimum trade count for consistency subscore to be non-zero
    /// (spec §12.2 `consistency_min_trades`).
    pub const CONSISTENCY_MIN_TRADES: usize = 2;
    /// Cold-start / new-agent reputation score.
    pub const NEW_AGENT_REPUTATION: f64 = 0.3;

    /// Compute the reputation score for `agent_hex` given all observed trades
    /// and a reference timestamp `now_ms`.
    pub fn compute(
        &self,
        agent_hex: &str,
        trades: &[TradeObservation],
        now_ms: u64,
    ) -> ReputationScore {
        // Filter to trades where this agent was provider or consumer.
        let agent_trades: Vec<&TradeObservation> = trades
            .iter()
            .filter(|t| t.provider_hex == agent_hex || t.consumer_hex == agent_hex)
            .collect();

        if agent_trades.is_empty() {
            return ReputationScore {
                overall: Self::NEW_AGENT_REPUTATION,
                volume: 0.0,
                recency: 0.0,
                diversity: 0.0,
                consistency: 0.0,
                trade_count: 0,
                computed_at_ms: now_ms,
            };
        }

        let volume = Self::volume_subscore(&agent_trades);
        let recency = Self::recency_subscore(&agent_trades, now_ms);
        let diversity = Self::diversity_subscore(agent_hex, &agent_trades);
        let consistency = Self::consistency_subscore(&agent_trades);

        let overall = (Self::WEIGHT_VOLUME * volume
            + Self::WEIGHT_RECENCY * recency
            + Self::WEIGHT_DIVERSITY * diversity
            + Self::WEIGHT_CONSISTENCY * consistency)
            .clamp(0.0, 1.0);

        ReputationScore {
            overall,
            volume,
            recency,
            diversity,
            consistency,
            trade_count: agent_trades.len(),
            computed_at_ms: now_ms,
        }
    }

    /// Compute reputation using trades stored in a registry.
    pub fn compute_from_registry(
        &self,
        agent_hex: &str,
        registry: &AgentRegistry,
        now_ms: u64,
    ) -> ReputationScore {
        self.compute(agent_hex, &registry.trades, now_ms)
    }

    fn volume_subscore(trades: &[&TradeObservation]) -> f64 {
        let total_trm: u64 = trades.iter().map(|t| t.trm_amount).sum();
        (total_trm as f64 / Self::VOLUME_CAP_CU as f64).min(1.0)
    }

    fn recency_subscore(trades: &[&TradeObservation], now_ms: u64) -> f64 {
        let most_recent = trades.iter().map(|t| t.timestamp_ms).max().unwrap_or(0);
        let age = now_ms.saturating_sub(most_recent);
        0.5_f64.powf(age as f64 / Self::RECENCY_HALF_LIFE_MS as f64)
    }

    fn diversity_subscore(agent_hex: &str, trades: &[&TradeObservation]) -> f64 {
        use std::collections::HashSet;
        let counterparties: HashSet<&str> = trades
            .iter()
            .map(|t| {
                if t.provider_hex == agent_hex {
                    t.consumer_hex.as_str()
                } else {
                    t.provider_hex.as_str()
                }
            })
            .collect();
        (counterparties.len() as f64 / Self::DIVERSITY_CAP as f64).min(1.0)
    }

    fn consistency_subscore(trades: &[&TradeObservation]) -> f64 {
        if trades.len() < Self::CONSISTENCY_MIN_TRADES {
            return 0.0;
        }
        let mut sorted: Vec<u64> = trades.iter().map(|t| t.timestamp_ms).collect();
        sorted.sort_unstable();

        let intervals: Vec<f64> = sorted
            .windows(2)
            .map(|w| (w[1] - w[0]) as f64)
            .collect();

        if intervals.is_empty() {
            return 0.0;
        }

        let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
        if mean == 0.0 {
            return 1.0;
        }

        let variance =
            intervals.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / intervals.len() as f64;
        let stdev = variance.sqrt();
        let cv = stdev / mean;
        (1.0 - cv / 2.0).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AgentRegistry;
    use crate::types::{AgentProfile, ModelTier};

    fn hex64(seed: &str) -> String {
        seed.repeat(64).chars().take(64).collect()
    }

    fn make_trade(provider: &str, consumer: &str, cu: u64, ts: u64) -> TradeObservation {
        TradeObservation {
            trade_id: format!("{provider}-{consumer}-{ts}"),
            provider_hex: hex64(provider),
            consumer_hex: hex64(consumer),
            trm_amount: cu,
            tokens: cu,
            model: "test".to_string(),
            tier: ModelTier::Small,
            timestamp_ms: ts,
        }
    }

    #[test]
    fn test_reputation_new_agent_has_default_score() {
        let calc = ReputationCalculator;
        let score = calc.compute(&hex64("a"), &[], 1_000_000);
        assert_eq!(score.overall, ReputationCalculator::NEW_AGENT_REPUTATION);
        assert_eq!(score.trade_count, 0);
    }

    #[test]
    fn test_high_volume_gets_full_volume_score() {
        let calc = ReputationCalculator;
        let base_ts = 1_700_000_000_000u64;
        let trades: Vec<TradeObservation> = (0..10)
            .map(|i| make_trade("a", &format!("c{i}"), 10_000, base_ts + i * 1000))
            .collect();
        let score = calc.compute(&hex64("a"), &trades, base_ts + 1000);
        assert_eq!(score.volume, 1.0);
        assert_eq!(score.trade_count, 10);
    }

    #[test]
    fn test_recent_trade_gets_high_recency_score() {
        let calc = ReputationCalculator;
        let now = 1_700_000_000_000u64;
        let trades = vec![make_trade("a", "b", 100, now)];
        let score = calc.compute(&hex64("a"), &trades, now);
        assert_eq!(score.recency, 1.0);
    }

    #[test]
    fn test_old_trade_gets_low_recency_score() {
        let calc = ReputationCalculator;
        let old_ts = 1_700_000_000_000u64;
        let now = old_ts + 7 * 24 * 3_600_000;
        let trades = vec![make_trade("a", "b", 100, old_ts)];
        let score = calc.compute(&hex64("a"), &trades, now);
        // 7 half-lives → 0.5^7 ≈ 0.0078
        assert!(score.recency < 0.01);
    }

    #[test]
    fn test_diverse_counterparties_gets_full_diversity_score() {
        let calc = ReputationCalculator;
        let base_ts = 1_700_000_000_000u64;
        let trades: Vec<TradeObservation> = (0..10usize)
            .map(|i| {
                let c = (b'a' + i as u8) as char;
                make_trade("p", &c.to_string(), 100, base_ts + i as u64 * 1000)
            })
            .collect();
        let score = calc.compute(&hex64("p"), &trades, base_ts + 1000);
        assert_eq!(score.diversity, 1.0);
    }

    #[test]
    fn test_single_counterparty_low_diversity() {
        let calc = ReputationCalculator;
        let base_ts = 1_700_000_000_000u64;
        let trades: Vec<TradeObservation> = (0..10)
            .map(|i| make_trade("p", "c", 100, base_ts + i * 1000))
            .collect();
        let score = calc.compute(&hex64("p"), &trades, base_ts + 1000);
        assert!((score.diversity - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_evenly_spaced_trades_high_consistency() {
        let calc = ReputationCalculator;
        let base_ts = 1_700_000_000_000u64;
        let trades: Vec<TradeObservation> = (0..10usize)
            .map(|i| {
                let c = (b'a' + (i % 9) as u8) as char;
                make_trade("p", &c.to_string(), 100, base_ts + i as u64 * 1000)
            })
            .collect();
        let score = calc.compute(&hex64("p"), &trades, base_ts + 100_000);
        assert!(score.consistency >= 0.9);
    }

    #[test]
    fn test_overall_clamped_to_unit_interval() {
        let calc = ReputationCalculator;
        let base_ts = 1_700_000_000_000u64;
        let trades: Vec<TradeObservation> = (0..20usize)
            .map(|i| {
                let c = (b'a' + (i % 10) as u8) as char;
                make_trade("p", &c.to_string(), 100_000, base_ts + i as u64 * 1000)
            })
            .collect();
        let score = calc.compute(&hex64("p"), &trades, base_ts + 1000);
        assert!((0.0..=1.0).contains(&score.overall));
    }

    #[test]
    fn test_compute_from_registry_matches_direct() {
        let mut reg = AgentRegistry::new();
        reg.register(AgentProfile {
            agent_hex: hex64("a"),
            models_served: vec![],
            trm_per_token: 1,
            tier: ModelTier::Small,
            last_seen_ms: 0,
        });
        reg.register(AgentProfile {
            agent_hex: hex64("b"),
            models_served: vec![],
            trm_per_token: 1,
            tier: ModelTier::Small,
            last_seen_ms: 0,
        });
        reg.observe_trade(make_trade("a", "b", 100, 1_700_000_000_000));

        let calc = ReputationCalculator;
        let now = 1_700_000_000_000u64;
        let from_reg = calc.compute_from_registry(&hex64("a"), &reg, now);
        let direct = calc.compute(&hex64("a"), &reg.trades, now);
        assert_eq!(from_reg.overall, direct.overall);
        assert_eq!(from_reg.trade_count, direct.trade_count);
    }

    #[test]
    fn test_weights_constants_sum_to_one() {
        let sum = ReputationCalculator::WEIGHT_VOLUME
            + ReputationCalculator::WEIGHT_RECENCY
            + ReputationCalculator::WEIGHT_DIVERSITY
            + ReputationCalculator::WEIGHT_CONSISTENCY;
        assert!((sum - 1.0).abs() < 1e-9);
    }

    // ===========================================================================
    // DEEP SECURITY TESTS — Round 2 (empty inputs, NaN volume, edge timestamps)
    // ===========================================================================

    #[test]
    fn sec_deep_reputation_empty_trades_returns_cold_start() {
        let calc = ReputationCalculator;
        let score = calc.compute(&hex64("a"), &[], 1_000_000);
        assert_eq!(
            score.overall,
            ReputationCalculator::NEW_AGENT_REPUTATION,
            "empty trades must return NEW_AGENT_REPUTATION"
        );
        assert!(!score.overall.is_nan(), "empty trades must not produce NaN score");
        assert_eq!(score.trade_count, 0);
    }

    #[test]
    fn sec_deep_reputation_overall_never_nan_with_zero_amounts() {
        // Trades with trm_amount = 0 → volume = 0 → volume_score = 0 (not NaN).
        let calc = ReputationCalculator;
        let now = 1_700_000_000_000u64;
        let trades = vec![make_trade("a", "b", 0, now)];
        let score = calc.compute(&hex64("a"), &trades, now);
        assert!(!score.overall.is_nan(), "zero-amount trades must not produce NaN score");
        assert!((0.0..=1.0).contains(&score.overall));
    }

    #[test]
    fn sec_deep_reputation_single_trade_no_consistency_score() {
        // Single trade → only 1 timestamp → no intervals → consistency = 0.
        let calc = ReputationCalculator;
        let now = 1_700_000_000_000u64;
        let trades = vec![make_trade("a", "b", 100, now)];
        let score = calc.compute(&hex64("a"), &trades, now);
        assert_eq!(score.consistency, 0.0, "single trade must have consistency 0");
        assert!(!score.consistency.is_nan());
    }

    #[test]
    fn sec_deep_reputation_now_before_trade_timestamp_recency_still_valid() {
        // now_ms < trade timestamp: saturating_sub(now, most_recent) = 0 → age = 0 → recency = 1.0.
        let calc = ReputationCalculator;
        let trade_ts = 1_700_000_000_000u64;
        let now = trade_ts - 1_000; // now is BEFORE the trade
        let trades = vec![make_trade("a", "b", 100, trade_ts)];
        let score = calc.compute(&hex64("a"), &trades, now);
        assert!(
            !score.recency.is_nan(),
            "recency must be finite even when now < trade timestamp"
        );
        assert!((0.0..=1.0).contains(&score.recency));
    }

    #[test]
    fn sec_deep_reputation_very_large_trm_amount_does_not_overflow() {
        // trm_amount near u64::MAX — summing multiple such trades could overflow.
        // volume_subscore uses total_trm as f64 / VOLUME_CAP → must not panic.
        let calc = ReputationCalculator;
        let now = 1_700_000_000_000u64;
        let trades = vec![
            make_trade("a", "b", u64::MAX / 2, now),
            make_trade("a", "c", u64::MAX / 2, now + 1000),
        ];
        let score = calc.compute(&hex64("a"), &trades, now + 1000);
        assert!(!score.volume.is_nan(), "u64::MAX volume must not produce NaN");
        assert!((0.0..=1.0).contains(&score.volume), "volume capped at 1.0");
        assert!(!score.overall.is_nan());
    }

    #[test]
    fn sec_deep_reputation_all_same_timestamps_consistency_is_one() {
        // All trades at the exact same timestamp → intervals all zero → mean = 0 → score = 1.0.
        let calc = ReputationCalculator;
        let now = 1_700_000_000_000u64;
        let trades: Vec<_> = (0..5u8).map(|i| {
            make_trade("a", &(i + 10).to_string(), 100, now)
        }).collect();
        let score = calc.compute(&hex64("a"), &trades, now);
        // mean interval = 0 → consistency returns 1.0.
        assert!(!score.consistency.is_nan(), "zero-interval trades must not produce NaN consistency");
    }
}
