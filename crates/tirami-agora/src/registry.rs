//! Agent registry — in-memory index of known agents and their trade history.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{AgentProfile, TradeObservation};

/// Serializable snapshot of the registry state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistrySnapshot {
    pub profiles: Vec<AgentProfile>,
    pub trades: Vec<TradeObservation>,
}

/// Local index of agents and their observed trade activity.
///
/// All state is held in memory. Use `snapshot()` / `restore()` for persistence.
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    pub profiles: HashMap<String, AgentProfile>,
    pub trades: Vec<TradeObservation>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
            trades: Vec::new(),
        }
    }

    /// Add or update an agent profile.
    pub fn register(&mut self, profile: AgentProfile) {
        self.profiles.insert(profile.agent_hex.clone(), profile);
    }

    /// Record an observed trade. Updates `last_seen_ms` for known parties.
    pub fn observe_trade(&mut self, trade: TradeObservation) {
        // Touch last_seen for provider if known.
        if let Some(p) = self.profiles.get_mut(&trade.provider_hex) {
            p.last_seen_ms = p.last_seen_ms.max(trade.timestamp_ms);
        }
        if let Some(p) = self.profiles.get_mut(&trade.consumer_hex) {
            p.last_seen_ms = p.last_seen_ms.max(trade.timestamp_ms);
        }
        self.trades.push(trade);
    }

    /// Look up a single agent by hex id.
    pub fn get_agent(&self, agent_hex: &str) -> Option<&AgentProfile> {
        self.profiles.get(agent_hex)
    }

    /// All registered agents.
    pub fn list_agents(&self) -> Vec<&AgentProfile> {
        self.profiles.values().collect()
    }

    /// Remove a profile, returning `true` if it existed.
    pub fn remove(&mut self, agent_hex: &str) -> bool {
        self.profiles.remove(agent_hex).is_some()
    }

    /// All trades where the node was provider or consumer.
    pub fn trades_for(&self, agent_hex: &str) -> Vec<&TradeObservation> {
        self.trades
            .iter()
            .filter(|t| t.provider_hex == agent_hex || t.consumer_hex == agent_hex)
            .collect()
    }

    /// All trades where the node was the provider.
    pub fn trades_as_provider(&self, agent_hex: &str) -> Vec<&TradeObservation> {
        self.trades
            .iter()
            .filter(|t| t.provider_hex == agent_hex)
            .collect()
    }

    /// Number of registered agents.
    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }

    /// Total recorded trades.
    pub fn trade_count(&self) -> usize {
        self.trades.len()
    }

    /// Serialize to a snapshot for persistence.
    pub fn snapshot(&self) -> RegistrySnapshot {
        RegistrySnapshot {
            profiles: self.profiles.values().cloned().collect(),
            trades: self.trades.clone(),
        }
    }

    /// Restore from a snapshot.
    pub fn restore(snapshot: RegistrySnapshot) -> Self {
        let mut profiles = HashMap::new();
        for profile in snapshot.profiles {
            profiles.insert(profile.agent_hex.clone(), profile);
        }
        Self {
            profiles,
            trades: snapshot.trades,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ModelTier;

    fn hex64(seed: &str) -> String {
        seed.repeat(64).chars().take(64).collect()
    }

    fn make_profile(seed: &str) -> AgentProfile {
        AgentProfile {
            agent_hex: hex64(seed),
            models_served: vec![format!("{seed}-model")],
            trm_per_token: 1,
            tier: ModelTier::Small,
            last_seen_ms: 1_700_000_000_000,
        }
    }

    fn make_trade(provider: &str, consumer: &str, cu: u64) -> TradeObservation {
        TradeObservation {
            trade_id: format!("{provider}-{consumer}"),
            provider_hex: hex64(provider),
            consumer_hex: hex64(consumer),
            trm_amount: cu,
            tokens: cu,
            model: "test".to_string(),
            tier: ModelTier::Small,
            timestamp_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = AgentRegistry::new();
        reg.register(make_profile("a"));
        let agent = reg.get_agent(&hex64("a")).unwrap();
        assert_eq!(agent.agent_hex, hex64("a"));
        assert_eq!(agent.models_served, vec!["a-model"]);
    }

    #[test]
    fn test_register_overwrites() {
        let mut reg = AgentRegistry::new();
        reg.register(make_profile("a"));
        let mut updated = make_profile("a");
        updated.trm_per_token = 5;
        reg.register(updated);
        assert_eq!(reg.get_agent(&hex64("a")).unwrap().trm_per_token, 5);
    }

    #[test]
    fn test_remove() {
        let mut reg = AgentRegistry::new();
        reg.register(make_profile("a"));
        assert!(reg.remove(&hex64("a")));
        assert!(reg.get_agent(&hex64("a")).is_none());
        assert!(!reg.remove(&hex64("a")));
    }

    #[test]
    fn test_observe_trade_increments_count() {
        let mut reg = AgentRegistry::new();
        reg.observe_trade(make_trade("a", "b", 10));
        reg.observe_trade(make_trade("a", "c", 10));
        assert_eq!(reg.trade_count(), 2);
    }

    #[test]
    fn test_trades_for_returns_relevant() {
        let mut reg = AgentRegistry::new();
        reg.observe_trade(make_trade("a", "b", 10));
        reg.observe_trade(make_trade("c", "a", 10));
        reg.observe_trade(make_trade("c", "d", 10));
        assert_eq!(reg.trades_for(&hex64("a")).len(), 2);
    }

    #[test]
    fn test_trades_as_provider() {
        let mut reg = AgentRegistry::new();
        reg.observe_trade(make_trade("a", "b", 10));
        reg.observe_trade(make_trade("c", "a", 10));
        let as_prov = reg.trades_as_provider(&hex64("a"));
        assert_eq!(as_prov.len(), 1);
        assert_eq!(as_prov[0].provider_hex, hex64("a"));
    }

    #[test]
    fn test_snapshot_round_trip() {
        let mut reg = AgentRegistry::new();
        let mut p = make_profile("a");
        p.tier = ModelTier::Medium;
        p.trm_per_token = 3;
        reg.register(p);
        reg.register(make_profile("b"));
        reg.observe_trade(make_trade("a", "c", 42));

        let snap = reg.snapshot();
        assert_eq!(snap.profiles.len(), 2);
        assert_eq!(snap.trades.len(), 1);

        let restored = AgentRegistry::restore(snap);
        assert_eq!(restored.profile_count(), 2);
        assert_eq!(restored.trade_count(), 1);
        let a = restored.get_agent(&hex64("a")).unwrap();
        assert_eq!(a.tier, ModelTier::Medium);
        assert_eq!(a.trm_per_token, 3);
    }

    #[test]
    fn test_observe_trade_updates_last_seen() {
        let mut reg = AgentRegistry::new();
        let mut profile = make_profile("a");
        profile.last_seen_ms = 1_000;
        reg.register(profile);
        let trade = TradeObservation {
            trade_id: "t1".to_string(),
            provider_hex: hex64("a"),
            consumer_hex: hex64("b"),
            trm_amount: 10,
            tokens: 10,
            model: "test".to_string(),
            tier: ModelTier::Small,
            timestamp_ms: 2_000,
        };
        reg.observe_trade(trade);
        assert_eq!(reg.get_agent(&hex64("a")).unwrap().last_seen_ms, 2_000);
    }
}
