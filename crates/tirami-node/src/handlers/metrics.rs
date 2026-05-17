//! /metrics endpoint — Prometheus / OpenMetrics export.
//!
//! Scrapes the current ledger state into a TiramiMetrics snapshot and returns
//! the Prometheus text exposition format (content-type: text/plain; version=0.0.4).
//!
//! This endpoint is intentionally NOT rate-limited because Prometheus typically
//! scrapes every 15-30 s and the metrics snapshot is cheap to compute.

use crate::api::{AppState, now_millis_pub};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use tirami_ledger::metrics::TiramiMetrics;
use std::sync::OnceLock;

fn metrics_instance() -> &'static TiramiMetrics {
    static INSTANCE: OnceLock<TiramiMetrics> = OnceLock::new();
    INSTANCE.get_or_init(TiramiMetrics::new)
}

/// Phase 25 A6 — process start timestamp, captured once on the
/// first /metrics scrape. We compute uptime_secs as
/// (now - process_started_at_secs) so dashboards can plot
/// node freshness and detect crash-loops.
fn process_started_at_secs() -> u64 {
    static STARTED_AT: OnceLock<u64> = OnceLock::new();
    *STARTED_AT.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    })
}

/// Phase 25 A3 — check whether the presented bearer satisfies the
/// metrics-protection policy. Returns `Ok(())` when the caller is
/// allowed to read /metrics, or `Err` with the status/body to send.
///
/// Policy:
///   - `config.metrics_require_bearer == false` (default): always allowed,
///     preserving Prometheus-friendly behaviour for private networks.
///   - `config.metrics_require_bearer == true`: the request must carry
///     a bearer that matches `config.api_bearer_token`. Without a token
///     configured the protection is meaningless, so the handler 503s
///     to surface the misconfiguration.
fn check_metrics_auth(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    if !state.config.metrics_require_bearer {
        return Ok(());
    }
    let Some(expected) = state.config.api_bearer_token.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "metrics_require_bearer is set but no api_bearer_token configured".to_string(),
        ));
    };
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if presented == Some(expected.as_str()) {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            "metrics endpoint requires bearer auth".to_string(),
        ))
    }
}

/// GET /metrics — Prometheus exposition format.
pub(crate) async fn metrics_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_metrics_auth(&state, &headers)?;
    let now_ms = now_millis_pub();
    let ledger = state.ledger.lock().await;
    let staking = state.staking_pool.lock().await;
    let referrals = state.referral_tracker.lock().await;
    let metrics = metrics_instance();
    metrics.observe_with_tokenomics(&ledger, now_ms, Some(&*staking), Some(&*referrals));
    drop(referrals);
    drop(staking);
    drop(ledger);
    let mut body = metrics
        .encode()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // Phase 25 A6 — append process-level gauges so dashboards can
    // plot node freshness without a sidecar exporter. Operators
    // tracking node fleet health graph `tirami_process_uptime_secs`
    // and alert on a regression toward 0 (crash-loop signature).
    let started_at = process_started_at_secs();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let uptime_secs = now_secs.saturating_sub(started_at);
    body.push_str("# HELP tirami_process_started_at_secs Unix epoch when this process began serving /metrics\n");
    body.push_str("# TYPE tirami_process_started_at_secs gauge\n");
    body.push_str(&format!("tirami_process_started_at_secs {started_at}\n"));
    body.push_str("# HELP tirami_process_uptime_secs Seconds since this process began serving /metrics\n");
    body.push_str("# TYPE tirami_process_uptime_secs gauge\n");
    body.push_str(&format!("tirami_process_uptime_secs {uptime_secs}\n"));
    body.push_str("# HELP tirami_protocol_version Wire protocol version this binary advertises\n");
    body.push_str("# TYPE tirami_protocol_version gauge\n");
    body.push_str(&format!(
        "tirami_protocol_version {}\n",
        tirami_core::TIRAMI_PROTOCOL_VERSION,
    ));
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

    // -----------------------------------------------------------------
    // Phase 25 A3 — metrics_require_bearer
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn metrics_default_is_open_for_prometheus_scrapers() {
        // metrics_require_bearer = false (default) — Prometheus
        // scraping behind a private boundary keeps working.
        let mut config = Config::default();
        config.api_bearer_token = Some("scrape-secret".to_string());
        let app = test_router_default(config);
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
    }

    #[tokio::test]
    async fn metrics_require_bearer_rejects_unauthenticated() {
        let mut config = Config::default();
        config.metrics_require_bearer = true;
        config.api_bearer_token = Some("scrape-secret".to_string());
        let app = test_router_default(config);
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
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn metrics_require_bearer_accepts_matching_token() {
        let mut config = Config::default();
        config.metrics_require_bearer = true;
        config.api_bearer_token = Some("scrape-secret".to_string());
        let app = test_router_default(config);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/metrics")
                    .header("Authorization", "Bearer scrape-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn metrics_require_bearer_rejects_wrong_token() {
        let mut config = Config::default();
        config.metrics_require_bearer = true;
        config.api_bearer_token = Some("scrape-secret".to_string());
        let app = test_router_default(config);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/metrics")
                    .header("Authorization", "Bearer wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn metrics_carries_phase_25_a6_process_gauges() {
        // Process gauges: uptime + started_at + protocol_version.
        // Operators graph these to detect crash-loops and version
        // skew across a fleet.
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&body);
        for expected in [
            "tirami_process_started_at_secs",
            "tirami_process_uptime_secs",
            "tirami_protocol_version",
        ] {
            assert!(
                text.contains(expected),
                "missing {expected} in /metrics output",
            );
        }
    }

    #[tokio::test]
    async fn metrics_require_bearer_without_token_returns_503_misconfig() {
        // If operator enables protection but forgets to configure the
        // token, fail loud rather than silently letting all traffic
        // through.
        let mut config = Config::default();
        config.metrics_require_bearer = true;
        config.api_bearer_token = None;
        let app = test_router_default(config);
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
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
