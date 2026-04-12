//! TrmPaidOptimizer — a MetaOptimizer that calls a frontier LLM via reqwest
//! and records the proposal's CU cost via the forge ledger (recording is
//! done by the forge-node handler after improve() returns; this crate stays
//! forge-ledger-independent).

use async_trait::async_trait;

use crate::harness::Harness;
use crate::meta_optimizer::MetaOptimizer;
use crate::types::{BenchmarkResult, ImprovementProposal};

/// A MetaOptimizer that calls a frontier LLM via HTTP to propose harness improvements.
///
/// The HTTP call is made asynchronously via reqwest. On any network or parse
/// error the optimizer falls back to a zero-cost no-change proposal so the
/// cycle runner can cleanly decide "Revert" without panicking.
pub struct TrmPaidOptimizer {
    /// Base URL of the frontier provider (e.g. "https://api.anthropic.com").
    pub api_url: String,
    /// Bearer token — never logged.
    pub api_key: String,
    /// Model id (e.g. "claude-sonnet-4-6", "gpt-4o").
    pub model: String,
    /// Fixed estimate used by the budget pre-gate. Real cost is computed
    /// from the actual response's token count.
    pub estimated_cu_per_call: u64,
    /// CU per output token, used to compute trm_cost_to_propose from
    /// the response's token count. Frontier tier defaults to ~20 CU/token
    /// per forge-economics §2.
    pub trm_per_output_token: u64,
}

impl TrmPaidOptimizer {
    pub fn new(
        api_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            api_url: api_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            estimated_cu_per_call: 10_000, // Conservative pre-gate
            trm_per_output_token: 20,        // Frontier tier
        }
    }
}

#[async_trait]
impl MetaOptimizer for TrmPaidOptimizer {
    fn name(&self) -> &str {
        "cu-paid-frontier"
    }

    fn estimated_trm_cost(&self) -> u64 {
        self.estimated_cu_per_call
    }

    async fn propose(&self, current: &Harness, baseline: &BenchmarkResult) -> ImprovementProposal {
        let prompt = format!(
            "You are improving an LLM harness. Current system prompt:\n---\n{}\n---\n\
             Baseline score: {:.3}\n\n\
             Propose a single concrete revision that should raise the score. \
             Respond with the new system prompt ONLY, no preamble.",
            current.system_prompt, baseline.score,
        );

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 1024,
            "messages": [{ "role": "user", "content": prompt }]
        });

        let endpoint = format!("{}/v1/messages", self.api_url.trim_end_matches('/'));
        let resp: serde_json::Value = match reqwest::Client::new()
            .post(&endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => match r.json::<serde_json::Value>().await {
                Ok(v) => v,
                Err(_) => return self.fallback_proposal(current, "json parse failed"),
            },
            Err(_) => return self.fallback_proposal(current, "http error"),
        };

        let new_prompt = resp
            .pointer("/content/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        if new_prompt.is_empty() {
            return self.fallback_proposal(current, "empty response");
        }

        let output_tokens = resp
            .pointer("/usage/output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(1024);

        let trm_cost = output_tokens.saturating_mul(self.trm_per_output_token);

        let proposed = current.evolve(
            Some(new_prompt.clone()),
            Some(format!("frontier proposal via {}", self.model)),
        );

        ImprovementProposal {
            proposed_harness: proposed,
            proposer_model: self.model.clone(),
            rationale: format!(
                "frontier proposal via {} ({} tokens)",
                self.model, output_tokens
            ),
            trm_cost_to_propose: trm_cost,
        }
    }
}

impl TrmPaidOptimizer {
    /// Zero-cost, no-change fallback so cycle runner can cleanly decide "Revert".
    fn fallback_proposal(&self, current: &Harness, reason: &str) -> ImprovementProposal {
        ImprovementProposal {
            proposed_harness: current.clone(),
            proposer_model: self.model.clone(),
            rationale: format!("fallback: {}", reason),
            trm_cost_to_propose: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Harness;
    use crate::types::BenchmarkResult;

    #[tokio::test]
    async fn cu_paid_optimizer_fallback_on_network_error() {
        // Use a deliberately invalid URL to exercise the fallback path.
        let opt = TrmPaidOptimizer::new("http://127.0.0.1:1", "fake-key", "claude-sonnet-4-6");
        let harness = Harness::new("test system prompt".to_string());
        let baseline = BenchmarkResult::new(1, 0.5, 1, 0, 0);
        let proposal = opt.propose(&harness, &baseline).await;
        // Fallback path: trm_cost is 0 and rationale mentions "fallback"
        assert_eq!(proposal.trm_cost_to_propose, 0);
        assert!(proposal.rationale.contains("fallback"));
    }

    #[tokio::test]
    async fn cu_paid_optimizer_fallback_returns_current_harness() {
        let opt = TrmPaidOptimizer::new("http://127.0.0.1:1", "fake-key", "claude-sonnet-4-6");
        let harness = Harness::new("original prompt".to_string());
        let baseline = BenchmarkResult::new(1, 0.5, 1, 0, 0);
        let proposal = opt.propose(&harness, &baseline).await;
        // Fallback should return a clone of the current harness (same version, same prompt)
        assert_eq!(proposal.proposed_harness.system_prompt, "original prompt");
        assert_eq!(proposal.proposed_harness.version, 1);
    }

    #[test]
    fn cu_paid_optimizer_estimated_trm_cost() {
        let opt = TrmPaidOptimizer::new("http://example.com", "key", "model");
        assert_eq!(opt.estimated_trm_cost(), 10_000);
        assert_eq!(opt.name(), "cu-paid-frontier");
    }
}
