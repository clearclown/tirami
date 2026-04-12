//! HTTP handlers for `/v1/tirami/mind/*` endpoints (Phase 8 / Batch B3).
//!
//! forge-mind has async code (MetaOptimizer::propose is async), so we
//! await improve() in the handler and then record CU costs in the ledger.

use axum::{Json, extract::State, http::StatusCode};
use tirami_mind::{
    TrmPaidOptimizer, EchoMetaOptimizer, TiramiMindAgent, Harness, InMemoryBenchmark,
    MindStats, PromptRewriteOptimizer,
};
use serde::{Deserialize, Serialize};

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};
use crate::mind_adapter::record_frontier_consumption;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MindInitRequest {
    pub system_prompt: String,
    /// "echo" | "prompt_rewrite" | "cu_paid"
    pub optimizer: String,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MindInitResponse {
    pub ok: bool,
    pub harness_version: u64,
}

#[derive(Debug, Serialize)]
pub struct MindStateResponse {
    pub harness_version: u64,
    pub system_prompt_preview: String,
    pub cycle_history_len: usize,
    pub budget_spent_today_cu: u64,
    pub budget_cycles_today: u32,
}

#[derive(Debug, Deserialize)]
pub struct MindImproveRequest {
    pub n_cycles: usize,
}

#[derive(Debug, Deserialize)]
pub struct MindBudgetRequest {
    #[serde(default)]
    pub max_trm_per_cycle: Option<u64>,
    #[serde(default)]
    pub max_trm_per_day: Option<u64>,
    #[serde(default)]
    pub max_cycles_per_day: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct MindBudgetResponse {
    pub ok: bool,
    pub max_trm_per_cycle: u64,
    pub max_trm_per_day: u64,
    pub max_cycles_per_day: u32,
}

#[derive(Debug, Serialize)]
pub struct MindStatsResponse {
    pub harness_version: u64,
    pub cycle_count: usize,
    pub kept: usize,
    pub reverted: usize,
    pub deferred: usize,
    pub total_trm_invested: u64,
    pub first_score: f64,
    pub latest_score: f64,
    pub score_delta: f64,
}

impl From<MindStats> for MindStatsResponse {
    fn from(s: MindStats) -> Self {
        Self {
            harness_version: s.harness_version,
            cycle_count: s.cycle_count,
            kept: s.kept,
            reverted: s.reverted,
            deferred: s.deferred,
            total_trm_invested: s.total_trm_invested,
            first_score: s.first_score,
            latest_score: s.latest_score,
            score_delta: s.score_delta,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn agent_not_initialized() -> (StatusCode, String) {
    (StatusCode::CONFLICT, "agent not initialized".to_string())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/tirami/mind/init
pub(crate) async fn mind_init(
    State(state): State<AppState>,
    Json(req): Json<MindInitRequest>,
) -> Result<Json<MindInitResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    let harness = Harness::new(req.system_prompt.clone());
    let benchmark = Box::new(InMemoryBenchmark::with_fn(|_| 0.5_f64));

    let optimizer: Box<dyn tirami_mind::MetaOptimizer> = match req.optimizer.as_str() {
        "echo" => Box::new(EchoMetaOptimizer),
        "prompt_rewrite" => Box::new(PromptRewriteOptimizer::with_fn(|p| {
            format!("{} Be concise and helpful.", p)
        })),
        "cu_paid" => {
            let api_url = req.api_url.unwrap_or_else(|| "https://api.anthropic.com".to_string());
            let api_key = req.api_key.unwrap_or_default();
            let model = req.model.unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            Box::new(TrmPaidOptimizer::new(api_url, api_key, model))
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown optimizer: {other}; expected echo|prompt_rewrite|cu_paid"),
            ));
        }
    };

    let mut agent = TiramiMindAgent::new(harness, benchmark, optimizer, None);

    // If a mind state path is configured and a snapshot file exists, restore
    // the harness + history + budget from disk. The optimizer and benchmark
    // are NOT restored (they were re-provided above by the caller).
    if let Some(ref path) = state.config.mind_state_path {
        match crate::state_persist::load_mind_snapshot(path) {
            Ok(Some(snap)) => {
                tracing::info!("Restoring mind agent snapshot from {}", path.display());
                agent.restore_from_snapshot(snap);
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Failed to load mind snapshot from {}: {}", path.display(), e);
            }
        }
    }

    let version = agent.harness.version;
    let mut mind = state.mind_agent.lock().await;
    *mind = Some(agent);

    Ok(Json(MindInitResponse {
        ok: true,
        harness_version: version,
    }))
}

/// GET /v1/tirami/mind/state
pub(crate) async fn mind_state(
    State(state): State<AppState>,
) -> Result<Json<MindStateResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let mind = state.mind_agent.lock().await;
    let agent = mind.as_ref().ok_or_else(agent_not_initialized)?;

    let preview: String = agent
        .harness
        .system_prompt
        .chars()
        .take(80)
        .collect();

    Ok(Json(MindStateResponse {
        harness_version: agent.harness.version,
        system_prompt_preview: preview,
        cycle_history_len: agent.cycle_count(),
        budget_spent_today_cu: agent.runner_budget().spent_today_cu,
        budget_cycles_today: agent.runner_budget().cycles_today,
    }))
}

/// POST /v1/tirami/mind/improve
pub(crate) async fn mind_improve(
    State(state): State<AppState>,
    Json(req): Json<MindImproveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    if req.n_cycles == 0 || req.n_cycles > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            "n_cycles must be between 1 and 100".to_string(),
        ));
    }

    let now_ms = now_millis_pub();
    let cycles = {
        let mut mind = state.mind_agent.lock().await;
        let agent = mind.as_mut().ok_or_else(agent_not_initialized)?;
        agent.improve(req.n_cycles, now_ms).await
    };

    // Record any frontier CU consumption in the ledger.
    for cycle in &cycles {
        let cu = cycle.proposal.trm_cost_to_propose;
        if cu > 0 {
            let model = &cycle.proposal.proposer_model;
            // Estimate tokens from CU / 20 (frontier tier rate)
            let tokens = cu / 20;
            record_frontier_consumption(&state.ledger, &state.local_node_id, model, tokens, cu)
                .await;
        }
    }

    let cycles_json = serde_json::to_value(&cycles)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "cycles_executed": cycles.len(),
        "cycles": cycles_json,
    })))
}

/// POST /v1/tirami/mind/budget
pub(crate) async fn mind_budget(
    State(state): State<AppState>,
    Json(req): Json<MindBudgetRequest>,
) -> Result<Json<MindBudgetResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let mut mind = state.mind_agent.lock().await;
    let agent = mind.as_mut().ok_or_else(agent_not_initialized)?;

    let budget = agent.runner_budget_mut();
    if let Some(v) = req.max_trm_per_cycle {
        budget.max_trm_per_cycle = v;
    }
    if let Some(v) = req.max_trm_per_day {
        budget.max_trm_per_day = v;
    }
    if let Some(v) = req.max_cycles_per_day {
        budget.max_cycles_per_day = v;
    }

    Ok(Json(MindBudgetResponse {
        ok: true,
        max_trm_per_cycle: budget.max_trm_per_cycle,
        max_trm_per_day: budget.max_trm_per_day,
        max_cycles_per_day: budget.max_cycles_per_day,
    }))
}

/// GET /v1/tirami/mind/stats
pub(crate) async fn mind_stats(
    State(state): State<AppState>,
) -> Result<Json<MindStatsResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let mind = state.mind_agent.lock().await;
    let agent = mind.as_ref().ok_or_else(agent_not_initialized)?;
    Ok(Json(agent.stats().into()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tirami_core::Config;
    use tower::util::ServiceExt;

    fn default_test_config() -> Config {
        Config::default()
    }

    #[tokio::test]
    async fn test_mind_init_echo() {
        let app = crate::api::test_router_default(default_test_config());
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tirami/mind/init")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"system_prompt":"hello world","optimizer":"echo"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_mind_init_then_state() {
        let app = crate::api::test_router_default(default_test_config());

        // POST /init with echo optimizer
        let init_req = Request::builder()
            .method("POST")
            .uri("/v1/tirami/mind/init")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"system_prompt":"hi","optimizer":"echo"}"#))
            .unwrap();
        let resp = app.clone().oneshot(init_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // GET /state
        let state_req = Request::builder()
            .method("GET")
            .uri("/v1/tirami/mind/state")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(state_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_mind_state_before_init_returns_409() {
        let app = crate::api::test_router_default(default_test_config());
        let req = Request::builder()
            .method("GET")
            .uri("/v1/tirami/mind/state")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_mind_improve_echo() {
        let app = crate::api::test_router_default(default_test_config());

        // Init first
        let init_req = Request::builder()
            .method("POST")
            .uri("/v1/tirami/mind/init")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"system_prompt":"hello","optimizer":"echo"}"#))
            .unwrap();
        app.clone().oneshot(init_req).await.unwrap();

        // POST /improve with 1 cycle
        let improve_req = Request::builder()
            .method("POST")
            .uri("/v1/tirami/mind/improve")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"n_cycles":1}"#))
            .unwrap();
        let resp = app.oneshot(improve_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_mind_stats_after_improve() {
        let app = crate::api::test_router_default(default_test_config());

        // Init
        let init_req = Request::builder()
            .method("POST")
            .uri("/v1/tirami/mind/init")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"system_prompt":"hi","optimizer":"echo"}"#))
            .unwrap();
        app.clone().oneshot(init_req).await.unwrap();

        // Improve
        let improve_req = Request::builder()
            .method("POST")
            .uri("/v1/tirami/mind/improve")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"n_cycles":2}"#))
            .unwrap();
        app.clone().oneshot(improve_req).await.unwrap();

        // Stats
        let stats_req = Request::builder()
            .method("GET")
            .uri("/v1/tirami/mind/stats")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(stats_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_mind_init_invalid_optimizer() {
        let app = crate::api::test_router_default(default_test_config());
        let req = Request::builder()
            .method("POST")
            .uri("/v1/tirami/mind/init")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"system_prompt":"hi","optimizer":"unknown"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
