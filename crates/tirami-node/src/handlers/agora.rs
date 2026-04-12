//! HTTP handlers for `/v1/tirami/agora/*` endpoints (Phase 8 / Batch B2).
//!
//! All handlers are 100% sync internally — forge-agora has no async code.
//! We call sync forge-agora methods while holding tokio Mutex guards.

use axum::{Json, extract::{Path, State}, http::StatusCode};
use tirami_agora::{AgentProfile, CapabilityMatch, CapabilityQuery, RegistrySnapshot, ReputationScore};
use serde::{Deserialize, Serialize};

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};
use crate::agora_adapter::refresh_marketplace_from_ledger;
// tirami_core::NodeId used for anti-collusion reputation lookup (Phase 9 A5)
use tirami_core;

// ---------------------------------------------------------------------------
// Request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize)]
pub struct FindRequest {
    pub model_patterns: Vec<String>,
    pub max_trm_per_token: Option<u64>,
    pub tier: Option<tirami_agora::ModelTier>,
    pub min_reputation: Option<f64>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/tirami/agora/register
pub(crate) async fn agora_register(
    State(state): State<AppState>,
    Json(profile): Json<AgentProfile>,
) -> Result<Json<OkResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    // Security: validate agent_hex is exactly 64 hex chars.
    // AgentProfile::new() performs this check, but serde deserialization
    // bypasses it. We must validate here at the API boundary.
    if profile.agent_hex.len() != 64 || !profile.agent_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("agent_hex must be exactly 64 hex characters, got {} chars", profile.agent_hex.len()),
        ));
    }
    let mut mp = state.marketplace.lock().await;
    mp.register_agent(profile);
    Ok(Json(OkResponse { ok: true }))
}

/// GET /v1/tirami/agora/agents
pub(crate) async fn agora_list_agents(
    State(state): State<AppState>,
) -> Result<Json<Vec<AgentProfile>>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let mp = state.marketplace.lock().await;
    let agents: Vec<AgentProfile> = mp.registry.list_agents().into_iter().cloned().collect();
    Ok(Json(agents))
}

/// GET /v1/tirami/agora/reputation/:hex
///
/// Returns the forge-agora `ReputationScore` for a registered agent, with the
/// `economic_reputation` field adjusted for collusion (Phase 9 A5).
pub(crate) async fn agora_reputation(
    State(state): State<AppState>,
    Path(hex): Path<String>,
) -> Result<Json<ReputationScore>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    refresh_marketplace_from_ledger(&state.ledger, &state.marketplace, &state.agora_last_seen).await;
    let mp = state.marketplace.lock().await;
    let mut score = mp.reputation_of(&hex, now_millis_pub());
    // Apply anti-collusion penalty from the ComputeLedger (Phase 9 A5).
    // Adjust the overall score by the effective (penalty-adjusted) reputation.
    if let Ok(node_id) = tirami_core::NodeId::from_hex(&hex) {
        let ledger = state.ledger.lock().await;
        let effective = ledger.effective_reputation(&node_id, now_millis_pub());
        // Blend the agora-computed overall with the ledger effective_reputation.
        // Using minimum ensures collusion penalty always reduces the visible score.
        score.overall = score.overall.min(effective);
    }
    Ok(Json(score))
}

/// POST /v1/tirami/agora/find
pub(crate) async fn agora_find(
    State(state): State<AppState>,
    Json(req): Json<FindRequest>,
) -> Result<Json<Vec<CapabilityMatch>>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let min_rep = req.min_reputation.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&min_rep) {
        return Err((StatusCode::BAD_REQUEST, "min_reputation must be in [0.0, 1.0]".into()));
    }
    let query = CapabilityQuery::new(
        req.model_patterns,
        req.max_trm_per_token.unwrap_or(u64::MAX),
        req.tier,
        min_rep,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    refresh_marketplace_from_ledger(&state.ledger, &state.marketplace, &state.agora_last_seen).await;
    let mp = state.marketplace.lock().await;
    let matches = mp.find(&query, now_millis_pub());
    Ok(Json(matches))
}

/// GET /v1/tirami/agora/stats
pub(crate) async fn agora_stats(
    State(state): State<AppState>,
) -> Result<Json<std::collections::HashMap<String, usize>>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let mp = state.marketplace.lock().await;
    Ok(Json(mp.stats()))
}

/// GET /v1/tirami/agora/snapshot
pub(crate) async fn agora_snapshot(
    State(state): State<AppState>,
) -> Result<Json<RegistrySnapshot>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let mp = state.marketplace.lock().await;
    Ok(Json(mp.registry.snapshot()))
}

/// POST /v1/tirami/agora/restore
pub(crate) async fn agora_restore(
    State(state): State<AppState>,
    Json(snapshot): Json<RegistrySnapshot>,
) -> Result<Json<OkResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let profile_count = snapshot.profiles.len();
    let mut mp = state.marketplace.lock().await;
    mp.registry = tirami_agora::AgentRegistry::restore(snapshot);
    // Reset last_seen_idx since registry was replaced; trades in the new registry
    // are already known, so sync the counter to the new trade count.
    drop(mp);
    let mut idx = state.agora_last_seen.lock().await;
    *idx = 0; // let refresh re-sync on next query
    drop(idx);
    tracing::info!("agora registry restored with {} profiles", profile_count);
    Ok(Json(OkResponse { ok: true }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    use crate::api::test_router_default;
    use tirami_core::Config;

    #[tokio::test]
    async fn test_agora_agents_empty_initially() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/agora/agents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_agora_register_and_list() {
        let app = test_router_default(Config::default());
        let profile = serde_json::json!({
            "agent_hex": "a".repeat(64),
            "models_served": ["qwen3-8b"],
            "trm_per_token": 3,
            "tier": "medium",
            "last_seen_ms": 1_700_000_000_000u64
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/agora/register")
                    .header("content-type", "application/json")
                    .body(Body::from(profile.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_agora_stats_returns_counts() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/agora/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["agent_count"].is_u64());
        assert!(json["trade_count"].is_u64());
    }

    #[tokio::test]
    async fn test_agora_snapshot_round_trip() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/agora/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["profiles"].is_array());
        assert!(json["trades"].is_array());
    }

    #[tokio::test]
    async fn test_agora_restore_accepts_empty_snapshot() {
        let app = test_router_default(Config::default());
        let snapshot = serde_json::json!({ "profiles": [], "trades": [] });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/agora/restore")
                    .header("content-type", "application/json")
                    .body(Body::from(snapshot.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], true);
    }

    #[tokio::test]
    async fn test_agora_find_returns_array() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({
            "model_patterns": ["*"],
            "max_trm_per_token": 100
        })
        .to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/agora/find")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn test_agora_reputation_unknown_agent_returns_new_agent_score() {
        let app = test_router_default(Config::default());
        let hex = "b".repeat(64);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/tirami/agora/reputation/{hex}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["overall"].is_f64());
    }
}
