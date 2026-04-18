//! HTTP handlers for `/v1/tirami/governance/*` endpoints (Phase 13).
//!
//! Stake-weighted governance: propose, vote, list proposals, tally results.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;
use tirami_ledger::ProposalKind;

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/tirami/governance/propose
///
/// Body: `{ "proposer": "hex64", "kind": "change_parameter"|"emergency_pause"|"protocol_upgrade",
///          "name": "...", "new_value": 1.0, "description": "...", "deadline_ms": 123456 }`
pub(crate) async fn governance_propose(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = check_forge_rate_limit(&state).await {
        return e.into_response();
    }

    let proposer_hex = match body["proposer"].as_str() {
        Some(s) if s.len() == 64 => s,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "proposer must be 64 hex chars" })),
            )
                .into_response();
        }
    };
    let mut proposer_bytes = [0u8; 32];
    if hex::decode_to_slice(proposer_hex, &mut proposer_bytes).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid proposer hex" })),
        )
            .into_response();
    }
    let proposer = tirami_core::NodeId(proposer_bytes);

    let kind_str = body["kind"].as_str().unwrap_or("");
    let kind = match kind_str {
        "change_parameter" => {
            let name = match body["name"].as_str() {
                Some(n) => n.to_string(),
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "error": "change_parameter requires 'name'" })),
                    )
                        .into_response();
                }
            };
            let new_value = match body["new_value"].as_f64() {
                Some(v) => v,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "error": "change_parameter requires 'new_value'" })),
                    )
                        .into_response();
                }
            };
            ProposalKind::ChangeParameter { name, new_value }
        }
        "emergency_pause" => ProposalKind::EmergencyPause,
        "protocol_upgrade" => {
            let description = body["description"]
                .as_str()
                .unwrap_or("")
                .to_string();
            ProposalKind::ProtocolUpgrade { description }
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "kind must be one of: change_parameter, emergency_pause, protocol_upgrade" })),
            )
                .into_response();
        }
    };

    let deadline_ms = match body["deadline_ms"].as_u64() {
        Some(d) => d,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "deadline_ms is required" })),
            )
                .into_response();
        }
    };

    let now_ms = now_millis_pub();
    let mut gov = state.governance.lock().await;
    match gov.create_proposal(proposer, kind, now_ms, deadline_ms) {
        Ok(id) => Json(json!({ "ok": true, "proposal_id": id })).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// POST /v1/tirami/governance/vote
///
/// Body: `{ "voter": "hex64", "proposal_id": 0, "approve": true,
///          "stake": 5000.0, "reputation": 0.8, "epochs_participated": 2 }`
pub(crate) async fn governance_vote(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = check_forge_rate_limit(&state).await {
        return e.into_response();
    }

    let voter_hex = match body["voter"].as_str() {
        Some(s) if s.len() == 64 => s,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "voter must be 64 hex chars" })),
            )
                .into_response();
        }
    };
    let mut voter_bytes = [0u8; 32];
    if hex::decode_to_slice(voter_hex, &mut voter_bytes).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid voter hex" })),
        )
            .into_response();
    }
    let voter = tirami_core::NodeId(voter_bytes);

    let proposal_id = match body["proposal_id"].as_u64() {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "proposal_id is required" })),
            )
                .into_response();
        }
    };

    let approve = match body["approve"].as_bool() {
        Some(b) => b,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "approve (bool) is required" })),
            )
                .into_response();
        }
    };

    let stake = match body["stake"].as_f64() {
        Some(s) => s as u64,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "stake is required" })),
            )
                .into_response();
        }
    };

    let reputation = match body["reputation"].as_f64() {
        Some(r) => r,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "reputation is required" })),
            )
                .into_response();
        }
    };

    let epochs_participated = body["epochs_participated"].as_u64().unwrap_or(0);

    let mut gov = state.governance.lock().await;
    match gov.cast_vote(voter, proposal_id, approve, stake, reputation, epochs_participated) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => {
            use tirami_ledger::GovernanceError;
            let status = match &e {
                GovernanceError::ProposalNotFound { .. } => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (status, Json(json!({ "error": e.to_string() }))).into_response()
        }
    }
}

/// GET /v1/tirami/governance/proposals
///
/// Returns all active proposals.
pub(crate) async fn governance_proposals(
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err(e) = check_forge_rate_limit(&state).await {
        return e.into_response();
    }

    let gov = state.governance.lock().await;
    let active: Vec<serde_json::Value> = gov
        .active_proposals()
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "proposer": hex::encode(p.proposer.0),
                "kind": format!("{:?}", p.kind),
                "epoch": p.epoch,
                "created_at_ms": p.created_at_ms,
                "deadline_ms": p.deadline_ms,
                "status": format!("{:?}", p.status),
            })
        })
        .collect();

    Json(json!({ "proposals": active })).into_response()
}

/// GET /v1/tirami/governance/tally/:id
///
/// Tallies votes for a proposal and returns the result.
pub(crate) async fn governance_tally(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    if let Err(e) = check_forge_rate_limit(&state).await {
        return e.into_response();
    }

    let mut gov = state.governance.lock().await;
    match gov.tally(id) {
        Ok(status) => Json(json!({
            "proposal_id": id,
            "status": format!("{:?}", status),
        }))
        .into_response(),
        Err(e) => {
            use tirami_ledger::GovernanceError;
            let code = match &e {
                GovernanceError::ProposalNotFound { .. } => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };
            (code, Json(json!({ "error": e.to_string() }))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    use crate::api::test_router_default;
    use tirami_core::Config;

    fn proposer_hex() -> String {
        "aa".repeat(32)
    }

    fn voter_hex() -> String {
        "bb".repeat(32)
    }

    #[tokio::test]
    async fn test_governance_propose_success() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({
            "proposer": proposer_hex(),
            "kind": "emergency_pause",
            "deadline_ms": 9_999_999_999u64,
        })
        .to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/propose")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"].as_bool().unwrap(), true);
        assert!(json["proposal_id"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_governance_vote_success() {
        let app = test_router_default(Config::default());

        // First, create a proposal
        let propose_body = serde_json::json!({
            "proposer": proposer_hex(),
            "kind": "emergency_pause",
            "deadline_ms": 9_999_999_999u64,
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/propose")
                    .header("content-type", "application/json")
                    .body(Body::from(propose_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let pjson: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let proposal_id = pjson["proposal_id"].as_u64().unwrap();

        // Now vote
        let vote_body = serde_json::json!({
            "voter": voter_hex(),
            "proposal_id": proposal_id,
            "approve": true,
            "stake": 5000,
            "reputation": 0.9,
            "epochs_participated": 2,
        })
        .to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/vote")
                    .header("content-type", "application/json")
                    .body(Body::from(vote_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"].as_bool().unwrap(), true);
    }

    #[tokio::test]
    async fn test_governance_proposals_empty() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/governance/proposals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["proposals"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_governance_proposals_with_data() {
        let app = test_router_default(Config::default());

        // Create a proposal
        let body = serde_json::json!({
            "proposer": proposer_hex(),
            "kind": "change_parameter",
            // Phase 18.1: must be a Constitutional whitelisted parameter.
            "name": "WELCOME_LOAN_AMOUNT",
            "new_value": 500.0,
            "deadline_ms": 9_999_999_999u64,
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/propose")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // List proposals
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/governance/proposals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["proposals"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_governance_tally_returns_correct_status() {
        let app = test_router_default(Config::default());

        // Create proposal
        let body = serde_json::json!({
            "proposer": proposer_hex(),
            "kind": "emergency_pause",
            "deadline_ms": 9_999_999_999u64,
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/propose")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let pjson: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let proposal_id = pjson["proposal_id"].as_u64().unwrap();

        // Vote approve
        let vote_body = serde_json::json!({
            "voter": voter_hex(),
            "proposal_id": proposal_id,
            "approve": true,
            "stake": 5000,
            "reputation": 0.9,
            "epochs_participated": 0,
        })
        .to_string();
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/vote")
                    .header("content-type", "application/json")
                    .body(Body::from(vote_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Tally
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&format!("/v1/tirami/governance/tally/{}", proposal_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"].as_str().unwrap(), "Passed");
    }

    #[tokio::test]
    async fn test_governance_vote_insufficient_reputation_returns_400() {
        let app = test_router_default(Config::default());

        // Create proposal
        let body = serde_json::json!({
            "proposer": proposer_hex(),
            "kind": "emergency_pause",
            "deadline_ms": 9_999_999_999u64,
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/propose")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let pjson: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let proposal_id = pjson["proposal_id"].as_u64().unwrap();

        // Vote with low reputation
        let vote_body = serde_json::json!({
            "voter": voter_hex(),
            "proposal_id": proposal_id,
            "approve": true,
            "stake": 5000,
            "reputation": 0.3,
            "epochs_participated": 0,
        })
        .to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/vote")
                    .header("content-type", "application/json")
                    .body(Body::from(vote_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_governance_vote_insufficient_stake_returns_400() {
        let app = test_router_default(Config::default());

        // Create proposal
        let body = serde_json::json!({
            "proposer": proposer_hex(),
            "kind": "emergency_pause",
            "deadline_ms": 9_999_999_999u64,
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/propose")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let pjson: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let proposal_id = pjson["proposal_id"].as_u64().unwrap();

        // Vote with low stake
        let vote_body = serde_json::json!({
            "voter": voter_hex(),
            "proposal_id": proposal_id,
            "approve": true,
            "stake": 100,
            "reputation": 0.9,
            "epochs_participated": 0,
        })
        .to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/vote")
                    .header("content-type", "application/json")
                    .body(Body::from(vote_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_governance_tally_nonexistent_returns_404() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/governance/tally/999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_governance_full_flow_propose_vote_tally() {
        let app = test_router_default(Config::default());

        // 1. Propose a parameter change
        let body = serde_json::json!({
            "proposer": proposer_hex(),
            "kind": "change_parameter",
            // Phase 18.1: Constitutional whitelist.
            "name": "ANCHOR_INTERVAL_SECS",
            "new_value": 900.0,
            "deadline_ms": 9_999_999_999u64,
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/propose")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let pjson: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let proposal_id = pjson["proposal_id"].as_u64().unwrap();

        // 2. Cast two votes: one approve (high stake), one reject (low stake)
        let vote1 = serde_json::json!({
            "voter": voter_hex(),
            "proposal_id": proposal_id,
            "approve": true,
            "stake": 10000,
            "reputation": 0.9,
            "epochs_participated": 3,
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/vote")
                    .header("content-type", "application/json")
                    .body(Body::from(vote1))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let voter2_hex = "cc".repeat(32);
        let vote2 = serde_json::json!({
            "voter": voter2_hex,
            "proposal_id": proposal_id,
            "approve": false,
            "stake": 2000,
            "reputation": 0.8,
            "epochs_participated": 0,
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/governance/vote")
                    .header("content-type", "application/json")
                    .body(Body::from(vote2))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 3. Tally — approve should win (10000*2.0=20000 vs 2000*1.0=2000)
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&format!("/v1/tirami/governance/tally/{}", proposal_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"].as_str().unwrap(), "Passed");
        assert_eq!(json["proposal_id"].as_u64().unwrap(), proposal_id);
    }
}
