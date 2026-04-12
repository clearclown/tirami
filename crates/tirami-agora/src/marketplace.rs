//! High-level marketplace facade combining registry, reputation, and matcher.

use std::collections::HashMap;

use crate::matching::CapabilityMatcher;
use crate::registry::AgentRegistry;
use crate::reputation::ReputationCalculator;
use crate::types::{AgentProfile, CapabilityMatch, CapabilityQuery, ReputationScore, TradeObservation};

/// The forge-agora marketplace facade.
///
/// Glues together `AgentRegistry`, `ReputationCalculator`, and `CapabilityMatcher`
/// into a single object for agent discovery.
pub struct Marketplace {
    pub registry: AgentRegistry,
    pub calculator: ReputationCalculator,
    pub matcher: CapabilityMatcher,
}

impl Default for Marketplace {
    fn default() -> Self {
        Self::new()
    }
}

impl Marketplace {
    pub fn new() -> Self {
        Self {
            registry: AgentRegistry::new(),
            calculator: ReputationCalculator,
            matcher: CapabilityMatcher,
        }
    }

    /// Register an agent profile.
    pub fn register_agent(&mut self, profile: AgentProfile) {
        self.registry.register(profile);
    }

    /// Record an observed trade and update agent last_seen.
    pub fn observe_trade(&mut self, trade: TradeObservation) {
        self.registry.observe_trade(trade);
    }

    /// Compute the reputation score for an agent at `now_ms`.
    pub fn reputation_of(&self, agent_hex: &str, now_ms: u64) -> ReputationScore {
        self.calculator
            .compute(agent_hex, &self.registry.trades, now_ms)
    }

    /// Find matching agents for a query, computing reputations at `now_ms`.
    pub fn find(&self, query: &CapabilityQuery, now_ms: u64) -> Vec<CapabilityMatch> {
        let agents: Vec<AgentProfile> = self.registry.list_agents().into_iter().cloned().collect();

        // Build reputation map for all registered agents.
        let reputations: HashMap<String, f64> = self
            .registry
            .profiles
            .keys()
            .map(|hex| {
                let score = self
                    .calculator
                    .compute(hex, &self.registry.trades, now_ms);
                (hex.clone(), score.overall)
            })
            .collect();

        self.matcher.find_matches(&agents, &reputations, query)
    }

    /// Summary stats.
    pub fn stats(&self) -> HashMap<String, usize> {
        let mut m = HashMap::new();
        m.insert("agent_count".to_string(), self.registry.profile_count());
        m.insert("trade_count".to_string(), self.registry.trade_count());
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ModelTier;

    fn hex64(seed: &str) -> String {
        seed.repeat(64).chars().take(64).collect()
    }

    fn make_profile(seed: &str, tier: ModelTier, cu: u64, models: Vec<&str>) -> AgentProfile {
        AgentProfile {
            agent_hex: hex64(seed),
            models_served: models.into_iter().map(|s| s.to_string()).collect(),
            trm_per_token: cu,
            tier,
            last_seen_ms: 1_700_000_000_000,
        }
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

    const BASE_TS: u64 = 1_700_000_000_000;

    #[test]
    fn test_register_and_find() {
        let mut mkt = Marketplace::new();
        mkt.register_agent(make_profile("a", ModelTier::Medium, 3, vec!["qwen3-8b"]));
        mkt.register_agent(make_profile("b", ModelTier::Medium, 3, vec!["llama-3-8b"]));

        let mut query = CapabilityQuery::default();
        query.model_patterns = vec!["qwen3-*".to_string()];
        let matches = mkt.find(&query, BASE_TS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].agent_hex, hex64("a"));
    }

    #[test]
    fn test_filter_by_tier() {
        let mut mkt = Marketplace::new();
        mkt.register_agent(make_profile(
            "small_a",
            ModelTier::Small,
            1,
            vec!["small-model"],
        ));
        mkt.register_agent(make_profile(
            "large_b",
            ModelTier::Large,
            8,
            vec!["large-model"],
        ));

        let mut query = CapabilityQuery::default();
        query.tier = Some(ModelTier::Large);
        let matches = mkt.find(&query, BASE_TS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].agent_hex, hex64("large_b"));
    }

    #[test]
    fn test_filter_by_max_price() {
        let mut mkt = Marketplace::new();
        mkt.register_agent(make_profile("cheap", ModelTier::Medium, 2, vec!["m"]));
        mkt.register_agent(make_profile("pricey", ModelTier::Medium, 10, vec!["m"]));

        let mut query = CapabilityQuery::default();
        query.max_trm_per_token = 5;
        let matches = mkt.find(&query, BASE_TS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].trm_per_token, 2);
    }

    #[test]
    fn test_higher_reputation_ranks_higher() {
        let mut mkt = Marketplace::new();
        mkt.register_agent(make_profile("popular", ModelTier::Medium, 3, vec!["m"]));
        mkt.register_agent(make_profile("new", ModelTier::Medium, 3, vec!["m"]));

        let base_ts = BASE_TS;
        for i in 0..10usize {
            let c = (b'a' + i as u8) as char;
            mkt.observe_trade(make_trade("popular", &c.to_string(), 10_000, base_ts + i as u64 * 1000));
        }

        let query = CapabilityQuery::default();
        let matches = mkt.find(&query, base_ts + 1000);
        assert_eq!(matches[0].agent_hex, hex64("popular"));
    }

    #[test]
    fn test_min_reputation_excludes_new_agents() {
        let mut mkt = Marketplace::new();
        mkt.register_agent(make_profile("new", ModelTier::Medium, 3, vec!["m"]));

        let mut query = CapabilityQuery::default();
        query.min_reputation = 0.5;
        let matches = mkt.find(&query, BASE_TS);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_reputation_of() {
        let mut mkt = Marketplace::new();
        mkt.register_agent(make_profile("a", ModelTier::Medium, 3, vec!["m"]));

        let score = mkt.reputation_of(&hex64("a"), BASE_TS);
        assert_eq!(score.overall, ReputationCalculator::NEW_AGENT_REPUTATION);
        assert_eq!(score.trade_count, 0);
    }

    #[test]
    fn test_stats() {
        let mut mkt = Marketplace::new();
        mkt.register_agent(make_profile("a", ModelTier::Medium, 3, vec!["m"]));
        mkt.register_agent(make_profile("b", ModelTier::Medium, 3, vec!["m"]));
        mkt.observe_trade(make_trade("a", "b", 10, BASE_TS));

        let stats = mkt.stats();
        assert_eq!(stats["agent_count"], 2);
        assert_eq!(stats["trade_count"], 1);
    }

    #[test]
    fn test_observe_trade_updates_last_seen() {
        let mut mkt = Marketplace::new();
        let mut profile = make_profile("a", ModelTier::Medium, 3, vec!["m"]);
        profile.last_seen_ms = 1_000;
        mkt.register_agent(profile);

        mkt.observe_trade(TradeObservation {
            trade_id: "t1".to_string(),
            provider_hex: hex64("a"),
            consumer_hex: hex64("b"),
            trm_amount: 10,
            tokens: 10,
            model: "m".to_string(),
            tier: ModelTier::Medium,
            timestamp_ms: 9_999,
        });

        assert_eq!(
            mkt.registry.get_agent(&hex64("a")).unwrap().last_seen_ms,
            9_999
        );
    }
}
