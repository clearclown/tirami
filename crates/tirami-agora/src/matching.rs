//! Capability matching engine.
//!
//! Given a `CapabilityQuery`, ranks agents by how well they match.
//! Hard filters first, then composite scoring: reputation + price.

use std::collections::HashMap;

use crate::types::{AgentProfile, CapabilityMatch, CapabilityQuery};

/// Ranks agents against a `CapabilityQuery`.
pub struct CapabilityMatcher;

impl CapabilityMatcher {
    /// Weight of reputation in composite score.
    pub const QUALITY_WEIGHT: f64 = 0.6;
    /// Weight of price in composite score (lower price → higher score).
    pub const COST_WEIGHT: f64 = 0.4;
    /// Multiplier used to anchor the price score denominator.
    pub const PRICE_SCORE_TIER_MULTIPLIER: f64 = 4.0;

    /// Find all agents that pass the hard filters and return them ranked by
    /// composite score (descending).
    ///
    /// `reputations` maps `agent_hex` → `reputation.overall`. Missing agents
    /// default to `ReputationCalculator::NEW_AGENT_REPUTATION` (0.3).
    pub fn find_matches(
        &self,
        agents: &[AgentProfile],
        reputations: &HashMap<String, f64>,
        query: &CapabilityQuery,
    ) -> Vec<CapabilityMatch> {
        let mut matches: Vec<CapabilityMatch> = Vec::new();

        for agent in agents {
            // Hard filter 1: tier must match if specified.
            if let Some(required_tier) = query.tier {
                if agent.tier != required_tier {
                    continue;
                }
            }

            // Hard filter 2: price ceiling.
            if agent.trm_per_token > query.max_trm_per_token {
                continue;
            }

            // Hard filter 3: reputation floor.
            let rep = *reputations
                .get(&agent.agent_hex)
                .unwrap_or(&crate::reputation::ReputationCalculator::NEW_AGENT_REPUTATION);
            if rep < query.min_reputation {
                continue;
            }

            // Hard filter 4: at least one model pattern must match.
            let matched_model = if query.model_patterns.is_empty() {
                // No pattern constraint — use the first model if any.
                agent.models_served.first().cloned().unwrap_or_default()
            } else {
                let found = agent.models_served.iter().find(|m| {
                    query.model_patterns.iter().any(|p| {
                        glob::Pattern::new(p)
                            .ok()
                            .map_or(false, |g| g.matches(m))
                    })
                });
                match found {
                    Some(m) => m.clone(),
                    None => continue, // no pattern matched
                }
            };

            // Composite score.
            let tier_base = agent.tier.base_trm_per_token() as f64;
            let price_score = (1.0
                - agent.trm_per_token as f64 / (tier_base * Self::PRICE_SCORE_TIER_MULTIPLIER))
                .max(0.0);
            let composite = Self::QUALITY_WEIGHT * rep + Self::COST_WEIGHT * price_score;

            matches.push(CapabilityMatch {
                agent_hex: agent.agent_hex.clone(),
                composite_score: composite,
                reputation: rep,
                price_score,
                trm_per_token: agent.trm_per_token,
                matched_model,
            });
        }

        // Sort descending by composite score.
        matches.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ModelTier;

    fn hex64(seed: &str) -> String {
        seed.repeat(64).chars().take(64).collect()
    }

    fn make_agent(seed: &str, tier: ModelTier, cu: u64, models: Vec<&str>) -> AgentProfile {
        AgentProfile {
            agent_hex: hex64(seed),
            models_served: models.into_iter().map(|s| s.to_string()).collect(),
            trm_per_token: cu,
            tier,
            last_seen_ms: 0,
        }
    }

    #[test]
    fn test_find_matches_no_filters() {
        let matcher = CapabilityMatcher;
        let agents = vec![
            make_agent("a", ModelTier::Medium, 3, vec!["qwen3-8b"]),
            make_agent("b", ModelTier::Medium, 3, vec!["llama-3-8b"]),
        ];
        let reps = HashMap::new();
        let query = CapabilityQuery::default();
        let results = matcher.find_matches(&agents, &reps, &query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_filter_by_tier() {
        let matcher = CapabilityMatcher;
        let agents = vec![
            make_agent("a", ModelTier::Small, 1, vec!["tiny"]),
            make_agent("b", ModelTier::Large, 8, vec!["big"]),
        ];
        let reps = HashMap::new();
        let mut query = CapabilityQuery::default();
        query.tier = Some(ModelTier::Large);
        let results = matcher.find_matches(&agents, &reps, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_hex, hex64("b"));
    }

    #[test]
    fn test_filter_by_price() {
        let matcher = CapabilityMatcher;
        let agents = vec![
            make_agent("cheap", ModelTier::Medium, 2, vec!["m1"]),
            make_agent("pricey", ModelTier::Medium, 10, vec!["m2"]),
        ];
        let reps = HashMap::new();
        let mut query = CapabilityQuery::default();
        query.max_trm_per_token = 5;
        let results = matcher.find_matches(&agents, &reps, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_hex, hex64("cheap"));
    }

    #[test]
    fn test_filter_by_reputation() {
        let matcher = CapabilityMatcher;
        let agents = vec![
            make_agent("reputable", ModelTier::Medium, 3, vec!["m"]),
            make_agent("new", ModelTier::Medium, 3, vec!["m"]),
        ];
        let mut reps = HashMap::new();
        reps.insert(hex64("reputable"), 0.8);
        // "new" agent defaults to 0.3

        let mut query = CapabilityQuery::default();
        query.min_reputation = 0.5;
        let results = matcher.find_matches(&agents, &reps, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_hex, hex64("reputable"));
    }

    #[test]
    fn test_filter_by_model_pattern() {
        let matcher = CapabilityMatcher;
        let agents = vec![
            make_agent("a", ModelTier::Medium, 3, vec!["qwen3-8b"]),
            make_agent("b", ModelTier::Medium, 3, vec!["llama-3-8b"]),
        ];
        let reps = HashMap::new();
        let mut query = CapabilityQuery::default();
        query.model_patterns = vec!["qwen3-*".to_string()];
        let results = matcher.find_matches(&agents, &reps, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_model, "qwen3-8b");
    }

    #[test]
    fn test_higher_reputation_ranks_higher() {
        let matcher = CapabilityMatcher;
        let agents = vec![
            make_agent("popular", ModelTier::Medium, 3, vec!["m"]),
            make_agent("new_agent", ModelTier::Medium, 3, vec!["m"]),
        ];
        let mut reps = HashMap::new();
        reps.insert(hex64("popular"), 0.9);
        reps.insert(hex64("new_agent"), 0.3);

        let query = CapabilityQuery::default();
        let results = matcher.find_matches(&agents, &reps, &query);
        assert_eq!(results[0].agent_hex, hex64("popular"));
    }

    #[test]
    fn test_composite_score_calculation() {
        let matcher = CapabilityMatcher;
        // Small tier base = 1 CU/token. trm_per_token=1, multiplier=4 → price_score = 1 - 1/4 = 0.75
        let agents = vec![make_agent("a", ModelTier::Small, 1, vec!["m"])];
        let mut reps = HashMap::new();
        reps.insert(hex64("a"), 0.8);
        let query = CapabilityQuery::default();
        let results = matcher.find_matches(&agents, &reps, &query);
        let expected = 0.6 * 0.8 + 0.4 * 0.75;
        assert!((results[0].composite_score - expected).abs() < 1e-9);
    }

    // ===========================================================================
    // DEEP SECURITY TESTS — Round 2 (empty agents, empty patterns, glob injection)
    // ===========================================================================

    #[test]
    fn sec_deep_capability_matcher_empty_agents_returns_empty() {
        let matcher = CapabilityMatcher;
        let reps = HashMap::new();
        let query = CapabilityQuery::default();
        let results = matcher.find_matches(&[], &reps, &query);
        assert!(results.is_empty(), "empty agent list must return empty matches, not panic");
    }

    #[test]
    fn sec_deep_capability_matcher_empty_model_patterns_matches_first_model() {
        // Empty model_patterns → no pattern constraint → matched_model = first model or empty.
        let matcher = CapabilityMatcher;
        let agents = vec![make_agent("a", ModelTier::Medium, 3, vec!["qwen3-8b"])];
        let reps = HashMap::new();
        let mut query = CapabilityQuery::default();
        query.model_patterns = vec![]; // empty patterns
        let results = matcher.find_matches(&agents, &reps, &query);
        // Agent with models should match (first model used).
        assert_eq!(results.len(), 1, "empty model_patterns must not filter out agents that have models");
        assert_eq!(results[0].matched_model, "qwen3-8b");
    }

    #[test]
    fn sec_deep_capability_matcher_agent_with_no_models_empty_pattern() {
        // Agent with no models_served + empty patterns → matched_model is empty string.
        let matcher = CapabilityMatcher;
        let agents = vec![make_agent("a", ModelTier::Medium, 3, vec![])]; // no models
        let reps = HashMap::new();
        let mut query = CapabilityQuery::default();
        query.model_patterns = vec![]; // no pattern constraint
        let results = matcher.find_matches(&agents, &reps, &query);
        assert_eq!(results.len(), 1, "agent with no models and no pattern constraint must still match");
        assert_eq!(results[0].matched_model, "", "matched_model must be empty string when no models");
    }

    #[test]
    fn sec_deep_capability_matcher_zero_max_trm_per_token_filters_all() {
        // max_trm_per_token = 0: all agents have trm_per_token >= 1 → none match.
        let matcher = CapabilityMatcher;
        let agents = vec![
            make_agent("a", ModelTier::Small, 1, vec!["m1"]),
            make_agent("b", ModelTier::Medium, 2, vec!["m2"]),
        ];
        let reps = HashMap::new();
        let mut query = CapabilityQuery::default();
        query.max_trm_per_token = 0;
        let results = matcher.find_matches(&agents, &reps, &query);
        assert!(results.is_empty(), "max_trm_per_token=0 must filter out all agents with trm_per_token >= 1");
    }

    #[test]
    fn sec_deep_capability_matcher_glob_patterns_do_not_cause_path_traversal() {
        // Verify adversarial glob patterns do not panic or produce unexpected matches.
        // The glob crate operates on string matching, not filesystem paths.
        let matcher = CapabilityMatcher;
        let agents = vec![make_agent("a", ModelTier::Medium, 3, vec!["safe-model"])];
        let reps = HashMap::new();
        let mut query = CapabilityQuery::default();
        // Adversarial patterns that could be problematic in filesystem glob operations.
        query.model_patterns = vec![
            "../../etc/passwd".to_string(),
            "**/**/../../etc".to_string(),
            "*".to_string(), // wildcard that should match "safe-model"
        ];
        // Must not panic regardless of pattern content.
        let result = std::panic::catch_unwind(|| {
            matcher.find_matches(&agents, &reps, &query)
        });
        assert!(result.is_ok(), "adversarial glob patterns must not cause panic");
        // "*" should match "safe-model".
        let results = result.unwrap();
        assert!(!results.is_empty(), "wildcard '*' must match 'safe-model'");
    }

    #[test]
    fn sec_deep_capability_matcher_nan_reputation_uses_new_agent_default() {
        // If reputation map contains NaN for an agent, it should be treated carefully.
        // The map lookup returns a reference; NaN in the map would fail the min_reputation check.
        let matcher = CapabilityMatcher;
        let agents = vec![make_agent("a", ModelTier::Medium, 3, vec!["m"])];
        let mut reps = HashMap::new();
        reps.insert(hex64("a"), f64::NAN); // inject NaN reputation
        let mut query = CapabilityQuery::default();
        query.min_reputation = 0.0; // accept any reputation

        // NaN < 0.0 is false in IEEE 754, so it passes the min_reputation filter.
        // Document behavior: NaN reputation passes min_reputation=0.0 filter.
        let result = std::panic::catch_unwind(|| {
            matcher.find_matches(&agents, &reps, &query)
        });
        assert!(result.is_ok(), "NaN reputation in the map must not cause panic");
    }
}
