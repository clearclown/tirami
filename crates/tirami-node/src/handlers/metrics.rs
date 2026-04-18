//! /metrics endpoint — Prometheus / OpenMetrics export.
//!
//! Scrapes the current ledger state into a TiramiMetrics snapshot and returns
//! the Prometheus text exposition format (content-type: text/plain; version=0.0.4).
//!
//! This endpoint is intentionally NOT rate-limited because Prometheus typically
//! scrapes every 15-30 s and the metrics snapshot is cheap to compute.

use crate::api::{AppState, now_millis_pub};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use tirami_ledger::metrics::TiramiMetrics;
use std::sync::OnceLock;

fn metrics_instance() -> &'static TiramiMetrics {
    static INSTANCE: OnceLock<TiramiMetrics> = OnceLock::new();
    INSTANCE.get_or_init(TiramiMetrics::new)
}

/// GET /metrics — Prometheus exposition format.
pub(crate) async fn metrics_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let now_ms = now_millis_pub();
    let ledger = state.ledger.lock().await;
    let staking = state.staking_pool.lock().await;
    let referrals = state.referral_tracker.lock().await;
    let metrics = metrics_instance();
    metrics.observe_with_tokenomics(&ledger, now_ms, Some(&*staking), Some(&*referrals));
    drop(referrals);
    drop(staking);
    drop(ledger);
    let body = metrics
        .encode()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    ))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tirami_core::Config;
    use tower::util::ServiceExt;

    use crate::api::test_router_default;

    #[tokio::test]
    async fn test_metrics_endpoint_returns_prometheus_format() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body =
            axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
        let text = String::from_utf8_lossy(&body);
        // Global counter always emits even when the ledger is empty (no nodes yet).
        assert!(
            text.contains("tirami_trade_count_total"),
            "missing tirami_trade_count_total in /metrics response:\n{text}"
        );
        assert!(
            text.contains("# TYPE"),
            "missing # TYPE line in /metrics response"
        );
    }
}
