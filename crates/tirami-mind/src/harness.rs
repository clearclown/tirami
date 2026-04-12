//! Harness — the unit of self-improvement.
//!
//! A harness is a complete agent configuration: system prompt, tool definitions,
//! sub-agent setup, and model routing strategy. The forge-mind self-improvement
//! loop mutates harnesses, benchmarks them, and decides whether to keep changes.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::errors::MindError;

/// A single tool the agent can use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema fragment
    pub parameters_schema: HashMap<String, serde_json::Value>,
}

/// Which model to use for which kind of task.
///
/// Routing key examples: 'default', 'reasoning', 'coding', 'translation'.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelStrategy {
    /// routing_key → model_id
    pub routes: HashMap<String, String>,
}

impl ModelStrategy {
    pub fn new(routes: HashMap<String, String>) -> Self {
        Self { routes }
    }

    pub fn default_strategy() -> Self {
        let mut routes = HashMap::new();
        routes.insert("default".to_string(), "qwen2.5:0.5b".to_string());
        Self { routes }
    }

    pub fn model_for(&self, routing_key: &str) -> String {
        self.routes
            .get(routing_key)
            .or_else(|| self.routes.get("default"))
            .cloned()
            .unwrap_or_default()
    }
}

/// Complete agent configuration; the unit of self-improvement.
///
/// Mutations should produce a NEW harness via `evolve()` rather than
/// mutating in place. This makes versioning, revert, and audit trivial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Harness {
    pub system_prompt: String,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    pub model_strategy: ModelStrategy,
    #[serde(default)]
    pub sub_agents: Vec<Harness>,
    pub version: u64,
    pub parent_version: Option<u64>,
    #[serde(default)]
    pub description: String,
}

impl Harness {
    /// Create a new harness with version 1 and no parent.
    pub fn new(system_prompt: String) -> Self {
        Self {
            system_prompt,
            tools: Vec::new(),
            model_strategy: ModelStrategy::default_strategy(),
            sub_agents: Vec::new(),
            version: 1,
            parent_version: None,
            description: String::new(),
        }
    }

    /// Produce a new harness with selected fields replaced.
    ///
    /// The new harness has `version = self.version + 1` and
    /// `parent_version = self.version`.
    pub fn evolve(&self, new_system_prompt: Option<String>, new_description: Option<String>) -> Self {
        Self {
            system_prompt: new_system_prompt.unwrap_or_else(|| self.system_prompt.clone()),
            tools: self.tools.clone(),
            model_strategy: self.model_strategy.clone(),
            sub_agents: self.sub_agents.clone(),
            version: self.version + 1,
            parent_version: Some(self.version),
            description: new_description.unwrap_or_else(|| self.description.clone()),
        }
    }

    /// Produce a new harness using a builder pattern.
    pub fn evolve_full(
        &self,
        system_prompt: Option<String>,
        tools: Option<Vec<ToolDefinition>>,
        model_strategy: Option<ModelStrategy>,
        sub_agents: Option<Vec<Harness>>,
        description: Option<String>,
    ) -> Self {
        Self {
            system_prompt: system_prompt.unwrap_or_else(|| self.system_prompt.clone()),
            tools: tools.unwrap_or_else(|| self.tools.clone()),
            model_strategy: model_strategy.unwrap_or_else(|| self.model_strategy.clone()),
            sub_agents: sub_agents.unwrap_or_else(|| self.sub_agents.clone()),
            version: self.version + 1,
            parent_version: Some(self.version),
            description: description.unwrap_or_else(|| self.description.clone()),
        }
    }

    pub fn to_json(&self) -> Result<String, MindError> {
        serde_json::to_string(self).map_err(MindError::Json)
    }

    pub fn from_json(raw: &str) -> Result<Self, MindError> {
        serde_json::from_str(raw).map_err(MindError::Json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_default_version() {
        let h = Harness::new("hi".to_string());
        assert_eq!(h.version, 1);
        assert!(h.parent_version.is_none());
    }

    #[test]
    fn test_evolve_bumps_version_and_sets_parent() {
        let h = Harness::new("v1".to_string());
        let h2 = h.evolve(Some("v2".to_string()), None);
        assert_eq!(h2.version, 2);
        assert_eq!(h2.parent_version, Some(1));
        assert_eq!(h2.system_prompt, "v2");
        // Original is untouched
        assert_eq!(h.version, 1);
        assert_eq!(h.system_prompt, "v1");
    }

    #[test]
    fn test_evolve_chained() {
        let h = Harness::new("v1".to_string());
        let h = h.evolve(Some("v2".to_string()), None);
        let h = h.evolve(Some("v3".to_string()), None);
        assert_eq!(h.version, 3);
        assert_eq!(h.parent_version, Some(2));
    }

    #[test]
    fn test_evolve_unspecified_fields_are_preserved() {
        let tool = ToolDefinition {
            name: "t".to_string(),
            description: "d".to_string(),
            parameters_schema: HashMap::new(),
        };
        let mut routes = HashMap::new();
        routes.insert("default".to_string(), "qwen-8b".to_string());
        let h = Harness {
            system_prompt: "hi".to_string(),
            tools: vec![tool.clone()],
            model_strategy: ModelStrategy::new(routes.clone()),
            sub_agents: vec![],
            version: 1,
            parent_version: None,
            description: "original".to_string(),
        };
        let h2 = h.evolve(Some("bye".to_string()), None);
        assert_eq!(h2.tools, vec![tool]);
        assert_eq!(h2.model_strategy.routes, routes);
        assert_eq!(h2.description, "original");
    }

    #[test]
    fn test_json_round_trip() {
        let mut routes = HashMap::new();
        routes.insert("default".to_string(), "qwen-8b".to_string());
        routes.insert("code".to_string(), "qwen-32b".to_string());
        let h = Harness {
            system_prompt: "hello".to_string(),
            tools: vec![ToolDefinition {
                name: "search".to_string(),
                description: "s".to_string(),
                parameters_schema: HashMap::new(),
            }],
            model_strategy: ModelStrategy::new(routes.clone()),
            sub_agents: vec![],
            version: 3,
            parent_version: Some(2),
            description: "test harness".to_string(),
        };
        let raw = h.to_json().unwrap();
        let h2 = Harness::from_json(&raw).unwrap();
        assert_eq!(h2.system_prompt, h.system_prompt);
        assert_eq!(h2.tools.len(), 1);
        assert_eq!(h2.tools[0].name, "search");
        assert_eq!(h2.model_strategy.routes, routes);
        assert_eq!(h2.version, 3);
        assert_eq!(h2.parent_version, Some(2));
    }

    #[test]
    fn test_to_json_is_valid_json() {
        let h = Harness::new("hi".to_string());
        let raw = h.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["system_prompt"], "hi");
        assert_eq!(parsed["version"], 1);
    }

    #[test]
    fn test_from_json_round_trip_preserves_version() {
        let h = Harness {
            system_prompt: "round-trip test".to_string(),
            tools: vec![],
            model_strategy: ModelStrategy::default_strategy(),
            sub_agents: vec![],
            version: 5,
            parent_version: Some(4),
            description: String::new(),
        };
        let raw = h.to_json().unwrap();
        let h2 = Harness::from_json(&raw).unwrap();
        assert_eq!(h2.system_prompt, h.system_prompt);
        assert_eq!(h2.version, 5);
        assert_eq!(h2.parent_version, Some(4));
    }

    #[test]
    fn test_model_strategy_default_fallback() {
        let mut routes = HashMap::new();
        routes.insert("default".to_string(), "qwen-8b".to_string());
        let s = ModelStrategy::new(routes);
        assert_eq!(s.model_for("default"), "qwen-8b");
        assert_eq!(s.model_for("nonexistent"), "qwen-8b");
    }

    #[test]
    fn test_model_strategy_routing() {
        let mut routes = HashMap::new();
        routes.insert("default".to_string(), "small".to_string());
        routes.insert("reasoning".to_string(), "large".to_string());
        let s = ModelStrategy::new(routes);
        assert_eq!(s.model_for("reasoning"), "large");
        assert_eq!(s.model_for("default"), "small");
    }
}
