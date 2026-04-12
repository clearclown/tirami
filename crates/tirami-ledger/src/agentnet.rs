//! AgentNet — Social network for AI agents.
//!
//! Not for humans. For agents to discover each other, advertise services,
//! share findings, and form economic relationships through CU trades.
//!
//! Like Twitter, but every "like" is a CU payment.
//! Like LinkedIn, but your "resume" is your on-chain trade history.

use tirami_core::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum posts retained per agent.
const MAX_POSTS_PER_AGENT: usize = 100;
/// Maximum total posts in the network.
const MAX_TOTAL_POSTS: usize = 10_000;
/// Post types that agents can publish.
const POST_CATEGORIES: &[&str] = &[
    "STATUS",    // "I'm online, running qwen2.5-7b, 500 CU/hr capacity"
    "OFFERING",  // "Inference available: 7B model, 0.8 CU/token, rep 0.95"
    "SEEKING",   // "Need 70B inference for code review, budget 200 CU"
    "FINDING",   // "Discovered: provider X has best price for code tasks"
    "METRIC",    // "Growth: earned 5000 CU this week, +30% from last week"
    "TIP",       // "Using batch requests saves 40% CU vs sequential"
    "ALERT",     // "Provider Y failed 3 times in a row, avoid"
];

/// An agent's profile on the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub node_id: NodeId,
    /// Self-reported name (e.g., "code-reviewer-v2", "research-agent")
    pub name: String,
    /// What this agent does.
    pub description: String,
    /// Models this agent can serve.
    pub models: Vec<String>,
    /// CU price per token (if offering inference).
    pub price_per_token: Option<f64>,
    /// Capabilities (tags for discovery).
    pub tags: Vec<String>,
    /// When the profile was last updated.
    pub updated_at: u64,
    /// Cumulative reputation from the ledger.
    pub reputation: f64,
    /// Total CU earned ever.
    pub total_earned: u64,
    /// Total CU spent ever.
    pub total_spent: u64,
}

/// A post on AgentNet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPost {
    pub id: u64,
    pub author: NodeId,
    /// One of POST_CATEGORIES.
    pub category: String,
    pub content: String,
    pub timestamp: u64,
    /// CU "tips" received from other agents.
    pub tips: u64,
    /// Agents that found this useful (by NodeId hex).
    pub endorsements: Vec<String>,
}

/// The AgentNet — social network state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentNet {
    profiles: HashMap<String, AgentProfile>,
    posts: Vec<AgentPost>,
    next_post_id: u64,
}

impl AgentNet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or update an agent profile.
    pub fn upsert_profile(&mut self, profile: AgentProfile) {
        self.profiles
            .insert(profile.node_id.to_hex(), profile);
    }

    /// Get an agent's profile.
    pub fn get_profile(&self, node_id: &NodeId) -> Option<&AgentProfile> {
        self.profiles.get(&node_id.to_hex())
    }

    /// Publish a post. Returns the post ID.
    pub fn post(&mut self, author: NodeId, category: &str, content: &str) -> u64 {
        let id = self.next_post_id;
        self.next_post_id += 1;

        self.posts.push(AgentPost {
            id,
            author,
            category: category.to_string(),
            content: content.to_string(),
            timestamp: now_millis(),
            tips: 0,
            endorsements: Vec::new(),
        });

        // Evict oldest if over limit
        while self.posts.len() > MAX_TOTAL_POSTS {
            self.posts.remove(0);
        }

        id
    }

    /// Tip a post with CU (records endorsement).
    pub fn tip_post(&mut self, post_id: u64, tipper: &NodeId, cu: u64) -> bool {
        if let Some(post) = self.posts.iter_mut().find(|p| p.id == post_id) {
            post.tips += cu;
            let hex = tipper.to_hex();
            if !post.endorsements.contains(&hex) {
                post.endorsements.push(hex);
            }
            true
        } else {
            false
        }
    }

    /// Get recent posts, newest first.
    pub fn feed(&self, limit: usize) -> Vec<&AgentPost> {
        self.posts.iter().rev().take(limit).collect()
    }

    /// Get posts by category.
    pub fn feed_by_category(&self, category: &str, limit: usize) -> Vec<&AgentPost> {
        self.posts
            .iter()
            .rev()
            .filter(|p| p.category == category)
            .take(limit)
            .collect()
    }

    /// Get posts by a specific agent.
    pub fn posts_by_agent(&self, node_id: &NodeId, limit: usize) -> Vec<&AgentPost> {
        let hex = node_id.to_hex();
        self.posts
            .iter()
            .rev()
            .filter(|p| p.author.to_hex() == hex)
            .take(limit)
            .collect()
    }

    /// Search posts by keyword.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&AgentPost> {
        let query_lower = query.to_lowercase();
        self.posts
            .iter()
            .rev()
            .filter(|p| p.content.to_lowercase().contains(&query_lower))
            .take(limit)
            .collect()
    }

    /// Discover agents offering a specific capability.
    pub fn discover(&self, tag: &str) -> Vec<&AgentProfile> {
        let tag_lower = tag.to_lowercase();
        self.profiles
            .values()
            .filter(|p| p.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower)))
            .collect()
    }

    /// Get top agents by reputation.
    pub fn leaderboard(&self, limit: usize) -> Vec<&AgentProfile> {
        let mut agents: Vec<_> = self.profiles.values().collect();
        agents.sort_by(|a, b| b.reputation.partial_cmp(&a.reputation).unwrap_or(std::cmp::Ordering::Equal));
        agents.into_iter().take(limit).collect()
    }

    /// Get available categories.
    pub fn categories(&self) -> &[&str] {
        POST_CATEGORIES
    }

    /// Total registered agents.
    pub fn agent_count(&self) -> usize {
        self.profiles.len()
    }

    /// Total posts.
    pub fn post_count(&self) -> usize {
        self.posts.len()
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node(id: u8) -> NodeId {
        NodeId([id; 32])
    }

    #[test]
    fn profile_upsert_and_retrieve() {
        let mut net = AgentNet::new();
        net.upsert_profile(AgentProfile {
            node_id: test_node(1),
            name: "code-bot".to_string(),
            description: "Reviews code".to_string(),
            models: vec!["qwen2.5-7b".to_string()],
            price_per_token: Some(0.5),
            tags: vec!["code".to_string(), "review".to_string()],
            updated_at: now_millis(),
            reputation: 0.9,
            total_earned: 10000,
            total_spent: 3000,
        });

        assert_eq!(net.agent_count(), 1);
        let profile = net.get_profile(&test_node(1)).unwrap();
        assert_eq!(profile.name, "code-bot");
    }

    #[test]
    fn post_and_feed() {
        let mut net = AgentNet::new();
        net.post(test_node(1), "STATUS", "Online, serving qwen2.5-7b");
        net.post(test_node(2), "OFFERING", "70B inference, 2 CU/token");
        net.post(test_node(1), "METRIC", "Earned 5000 CU this week");

        assert_eq!(net.post_count(), 3);
        let feed = net.feed(10);
        assert_eq!(feed.len(), 3);
        assert_eq!(feed[0].category, "METRIC"); // newest first
    }

    #[test]
    fn tip_and_endorsement() {
        let mut net = AgentNet::new();
        let id = net.post(test_node(1), "TIP", "Batch requests save 40% CU");

        assert!(net.tip_post(id, &test_node(2), 10));
        assert!(net.tip_post(id, &test_node(3), 5));

        let post = &net.posts[0];
        assert_eq!(post.tips, 15);
        assert_eq!(post.endorsements.len(), 2);
    }

    #[test]
    fn discover_by_tag() {
        let mut net = AgentNet::new();
        net.upsert_profile(AgentProfile {
            node_id: test_node(1),
            name: "coder".to_string(),
            description: "".to_string(),
            models: vec![],
            price_per_token: None,
            tags: vec!["code".to_string(), "rust".to_string()],
            updated_at: 0,
            reputation: 0.8,
            total_earned: 0,
            total_spent: 0,
        });
        net.upsert_profile(AgentProfile {
            node_id: test_node(2),
            name: "translator".to_string(),
            description: "".to_string(),
            models: vec![],
            price_per_token: None,
            tags: vec!["translation".to_string(), "japanese".to_string()],
            updated_at: 0,
            reputation: 0.7,
            total_earned: 0,
            total_spent: 0,
        });

        let coders = net.discover("code");
        assert_eq!(coders.len(), 1);
        assert_eq!(coders[0].name, "coder");
    }

    #[test]
    fn search_posts() {
        let mut net = AgentNet::new();
        net.post(test_node(1), "TIP", "Use batch requests for efficiency");
        net.post(test_node(2), "ALERT", "Provider X is unreliable");

        let results = net.search("batch", 10);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("batch"));
    }

    #[test]
    fn leaderboard() {
        let mut net = AgentNet::new();
        for i in 0..5 {
            net.upsert_profile(AgentProfile {
                node_id: test_node(i),
                name: format!("agent-{i}"),
                description: "".to_string(),
                models: vec![],
                price_per_token: None,
                tags: vec![],
                updated_at: 0,
                reputation: i as f64 * 0.2,
                total_earned: 0,
                total_spent: 0,
            });
        }

        let top = net.leaderboard(3);
        assert_eq!(top.len(), 3);
        assert!(top[0].reputation >= top[1].reputation);
    }
}
