//! Core data types for forge-agora.
//!
//! Mirrors `forge-economics/spec/parameters.md` §12.
//! `ModelTier` is re-exported from `tirami_ledger::lending` — not redefined here.

use serde::{Deserialize, Serialize};

use crate::errors::AgoraError;

// Re-export so downstream users can access ModelTier via tirami_agora.
pub use tirami_ledger::lending::ModelTier;

/// A single trade observation from the gossip stream.
///
/// Both signatures must have been verified upstream before construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeObservation {
    pub trade_id: String,
    pub provider_hex: String,
    pub consumer_hex: String,
    pub trm_amount: u64,
    pub tokens: u64,
    pub model: String,
    pub tier: ModelTier,
    pub timestamp_ms: u64,
}

impl TradeObservation {
    pub fn new(
        trade_id: String,
        provider_hex: String,
        consumer_hex: String,
        trm_amount: u64,
        tokens: u64,
        model: String,
        tier: ModelTier,
        timestamp_ms: u64,
    ) -> Result<Self, AgoraError> {
        if provider_hex.len() != 64 {
            return Err(AgoraError::InvalidHex(format!(
                "provider_hex must be 64 chars, got {}",
                provider_hex.len()
            )));
        }
        if consumer_hex.len() != 64 {
            return Err(AgoraError::InvalidHex(format!(
                "consumer_hex must be 64 chars, got {}",
                consumer_hex.len()
            )));
        }
        if provider_hex == consumer_hex {
            return Err(AgoraError::InvalidTrade(
                "provider and consumer must differ".to_string(),
            ));
        }
        if trm_amount == 0 {
            return Err(AgoraError::InvalidTrade(
                "trm_amount must be > 0".to_string(),
            ));
        }
        Ok(Self {
            trade_id,
            provider_hex,
            consumer_hex,
            trm_amount,
            tokens,
            model,
            tier,
            timestamp_ms,
        })
    }
}

/// A discovered agent in the marketplace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub agent_hex: String,
    pub models_served: Vec<String>,
    pub trm_per_token: u64,
    pub tier: ModelTier,
    pub last_seen_ms: u64,
}

impl AgentProfile {
    pub fn new(
        agent_hex: String,
        models_served: Vec<String>,
        trm_per_token: u64,
        tier: ModelTier,
        last_seen_ms: u64,
    ) -> Result<Self, AgoraError> {
        if agent_hex.len() != 64 {
            return Err(AgoraError::InvalidHex(format!(
                "agent_hex must be 64 chars, got {}",
                agent_hex.len()
            )));
        }
        if trm_per_token == 0 {
            return Err(AgoraError::InvalidTrade(
                "trm_per_token must be >= 1".to_string(),
            ));
        }
        Ok(Self {
            agent_hex,
            models_served,
            trm_per_token,
            tier,
            last_seen_ms,
        })
    }
}

/// A locally computed reputation score for a single agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReputationScore {
    pub overall: f64,
    pub volume: f64,
    pub recency: f64,
    pub diversity: f64,
    pub consistency: f64,
    pub trade_count: usize,
    pub computed_at_ms: u64,
}

/// A query for matching agents to a workload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityQuery {
    pub model_patterns: Vec<String>,
    pub max_trm_per_token: u64,
    pub tier: Option<ModelTier>,
    pub min_reputation: f64,
}

impl Default for CapabilityQuery {
    fn default() -> Self {
        Self {
            model_patterns: vec![],
            max_trm_per_token: u64::MAX,
            tier: None,
            min_reputation: 0.0,
        }
    }
}

impl CapabilityQuery {
    pub fn new(
        model_patterns: Vec<String>,
        max_trm_per_token: u64,
        tier: Option<ModelTier>,
        min_reputation: f64,
    ) -> Result<Self, AgoraError> {
        if !(0.0..=1.0).contains(&min_reputation) {
            return Err(AgoraError::InvalidQuery(format!(
                "min_reputation must be in [0.0, 1.0], got {min_reputation}"
            )));
        }
        Ok(Self {
            model_patterns,
            max_trm_per_token,
            tier,
            min_reputation,
        })
    }
}

/// A ranked match returned by the marketplace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityMatch {
    pub agent_hex: String,
    pub composite_score: f64,
    pub reputation: f64,
    pub price_score: f64,
    pub trm_per_token: u64,
    pub matched_model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex64(seed: &str) -> String {
        seed.repeat(64).chars().take(64).collect()
    }

    #[test]
    fn test_model_tier_from_active_params() {
        assert_eq!(ModelTier::from_active_params(500_000_000), ModelTier::Small);
        assert_eq!(
            ModelTier::from_active_params(8_000_000_000),
            ModelTier::Medium
        );
        assert_eq!(
            ModelTier::from_active_params(32_000_000_000),
            ModelTier::Large
        );
        assert_eq!(
            ModelTier::from_active_params(405_000_000_000),
            ModelTier::Frontier
        );
    }

    #[test]
    fn test_model_tier_base_prices_match_spec() {
        assert_eq!(ModelTier::Small.base_trm_per_token(), 1);
        assert_eq!(ModelTier::Medium.base_trm_per_token(), 3);
        assert_eq!(ModelTier::Large.base_trm_per_token(), 8);
        assert_eq!(ModelTier::Frontier.base_trm_per_token(), 20);
    }

    #[test]
    fn test_trade_observation_validates_provider_hex_length() {
        let result = TradeObservation::new(
            "id".to_string(),
            "abc".to_string(),
            hex64("b"),
            10,
            10,
            "test".to_string(),
            ModelTier::Small,
            1000,
        );
        assert!(matches!(result, Err(AgoraError::InvalidHex(_))));
    }

    #[test]
    fn test_trade_observation_validates_consumer_hex_length() {
        let result = TradeObservation::new(
            "id".to_string(),
            hex64("a"),
            "abc".to_string(),
            10,
            10,
            "test".to_string(),
            ModelTier::Small,
            1000,
        );
        assert!(matches!(result, Err(AgoraError::InvalidHex(_))));
    }

    #[test]
    fn test_trade_observation_rejects_zero_cu() {
        let result = TradeObservation::new(
            "id".to_string(),
            hex64("a"),
            hex64("b"),
            0,
            10,
            "test".to_string(),
            ModelTier::Small,
            1000,
        );
        assert!(matches!(result, Err(AgoraError::InvalidTrade(_))));
    }

    #[test]
    fn test_trade_observation_rejects_same_provider_consumer() {
        let result = TradeObservation::new(
            "id".to_string(),
            hex64("a"),
            hex64("a"),
            10,
            10,
            "test".to_string(),
            ModelTier::Small,
            1000,
        );
        assert!(matches!(result, Err(AgoraError::InvalidTrade(_))));
    }

    #[test]
    fn test_trade_observation_valid_round_trip() {
        let trade = TradeObservation::new(
            "id1".to_string(),
            hex64("a"),
            hex64("b"),
            42,
            42,
            "qwen3-8b".to_string(),
            ModelTier::Medium,
            1_700_000_000_000,
        )
        .unwrap();
        assert_eq!(trade.trm_amount, 42);
        assert_eq!(trade.model, "qwen3-8b");
    }

    #[test]
    fn test_agent_profile_validates_hex_length() {
        let result = AgentProfile::new(
            "too-short".to_string(),
            vec![],
            1,
            ModelTier::Small,
            0,
        );
        assert!(matches!(result, Err(AgoraError::InvalidHex(_))));
    }

    #[test]
    fn test_capability_query_default() {
        let q = CapabilityQuery::default();
        assert_eq!(q.min_reputation, 0.0);
        assert_eq!(q.max_trm_per_token, u64::MAX);
        assert!(q.model_patterns.is_empty());
        assert!(q.tier.is_none());
    }
}
