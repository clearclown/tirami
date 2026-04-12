//! /v1/tirami/anchor — build an OP_RETURN anchor for the current trade Merkle root.

use crate::api::{AppState, check_forge_rate_limit};
use axum::extract::{Query, State};
use axum::Json;
use axum::http::StatusCode;
use bitcoin::Network;
use tirami_ledger::anchor::AnchorRequest;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct AnchorQuery {
    /// Network: "mainnet" | "testnet" | "signet" | "regtest" (default: "testnet")
    #[serde(default = "default_network")]
    pub network: String,
}

fn default_network() -> String {
    "testnet".to_string()
}

pub(crate) async fn anchor_handler(
    State(state): State<AppState>,
    Query(q): Query<AnchorQuery>,
) -> Result<Json<AnchorRequest>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    let net = match q.network.to_lowercase().as_str() {
        "mainnet" | "bitcoin" => Network::Bitcoin,
        "testnet" => Network::Testnet,
        "signet" => Network::Signet,
        "regtest" => Network::Regtest,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown network: {}", other),
            ))
        }
    };

    let root = {
        let ledger = state.ledger.lock().await;
        ledger.compute_trade_merkle_root()
    };

    Ok(Json(AnchorRequest::new(&root, net)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    use crate::api::test_router_default;
    use tirami_core::Config;

    #[tokio::test]
    async fn test_anchor_endpoint_returns_payload() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/anchor?network=testnet")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let req: AnchorRequest = serde_json::from_slice(&body).unwrap();
        assert_eq!(req.payload_len, 40);
        assert_eq!(req.merkle_root_hex.len(), 64);
        assert!(req.script_hex.starts_with("6a28")); // OP_RETURN + 0x28 push
    }

    #[tokio::test]
    async fn test_anchor_endpoint_rejects_unknown_network() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/anchor?network=dogecoin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
