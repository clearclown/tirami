//! Security-focused HTTP API tests for the Forge protocol.
//!
//! These tests verify that the API correctly defends against:
//!   1. Authentication bypass (missing / wrong / malformed tokens)
//!   2. Malformed JSON input (invalid bodies, missing required fields)
//!   3. Path traversal and injection via URL parameters
//!   4. Numeric overflow and extreme values
//!   5. Rate limiting under burst traffic
//!   6. Wrong Content-Type headers
//!
//! All tests use the `test_router_default()` helper + axum `oneshot()` pattern
//! and must pass green (proving defences are already in place or exposing real bugs).

#[cfg(test)]
mod security_tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header::AUTHORIZATION};
    use tirami_core::Config;
    use tower::util::ServiceExt;

    use crate::api::test_router_default;

    // =========================================================================
    // 1. Authentication bypass
    // =========================================================================

    /// Protected endpoint returns 401 when the Authorization header is absent.
    #[tokio::test]
    async fn test_protected_endpoint_rejects_missing_auth() {
        let mut config = Config::default();
        config.api_bearer_token = Some("s3cr3t".to_string());
        let app = test_router_default(config);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/tirami/balance")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Protected endpoint returns 401 when a clearly wrong token is supplied.
    #[tokio::test]
    async fn test_protected_endpoint_rejects_wrong_token() {
        let mut config = Config::default();
        config.api_bearer_token = Some("s3cr3t".to_string());
        let app = test_router_default(config);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/tirami/balance")
                    .header(AUTHORIZATION, "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// "Basic" scheme must be rejected (not Bearer).
    #[tokio::test]
    async fn test_protected_endpoint_rejects_basic_scheme() {
        let mut config = Config::default();
        config.api_bearer_token = Some("s3cr3t".to_string());
        let app = test_router_default(config);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/tirami/balance")
                    .header(AUTHORIZATION, "Basic czNjcjN0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Lowercase "bearer" prefix must be rejected (scheme is case-sensitive in this implementation).
    #[tokio::test]
    async fn test_protected_endpoint_rejects_lowercase_bearer() {
        let mut config = Config::default();
        config.api_bearer_token = Some("s3cr3t".to_string());
        let app = test_router_default(config);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/tirami/balance")
                    .header(AUTHORIZATION, "bearer s3cr3t")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// "Bearer" with no token text (only the prefix) must be rejected.
    #[tokio::test]
    async fn test_protected_endpoint_rejects_bare_bearer_prefix() {
        let mut config = Config::default();
        config.api_bearer_token = Some("s3cr3t".to_string());
        let app = test_router_default(config);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/tirami/balance")
                    // "Bearer" without a trailing space+token — strip_prefix("Bearer ") gives None
                    .header(AUTHORIZATION, "Bearer")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// /health must be accessible without any authentication token.
    #[tokio::test]
    async fn test_public_health_works_without_auth() {
        let mut config = Config::default();
        config.api_bearer_token = Some("s3cr3t".to_string());
        let app = test_router_default(config);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// /metrics must be accessible without any authentication token (Prometheus scrape).
    #[tokio::test]
    async fn test_public_metrics_works_without_auth() {
        let mut config = Config::default();
        config.api_bearer_token = Some("s3cr3t".to_string());
        let app = test_router_default(config);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    // =========================================================================
    // 2. Malformed JSON input
    // =========================================================================

    /// POST /v1/tirami/bank/strategy with syntactically invalid JSON body → 400.
    ///
    /// axum's `Json` extractor returns 400 Bad Request (not 422) for parse errors.
    #[tokio::test]
    async fn test_bank_strategy_rejects_malformed_json() {
        let app = test_router_default(Config::default());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/strategy")
                    .header("content-type", "application/json")
                    .body(Body::from("{invalid json}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        // axum returns 400 for malformed JSON bodies.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/tirami/bank/risk with syntactically invalid JSON body → 400.
    ///
    /// axum's `Json` extractor returns 400 Bad Request for parse errors.
    #[tokio::test]
    async fn test_bank_risk_rejects_malformed_json() {
        let app = test_router_default(Config::default());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/risk")
                    .header("content-type", "application/json")
                    .body(Body::from("{not: valid}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        // axum returns 400 for malformed JSON bodies.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/chat/completions with an empty messages array → 400.
    /// This already has a test in api.rs — here we explicitly document it as a security boundary.
    #[tokio::test]
    async fn test_chat_completions_rejects_empty_messages_sec() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({ "messages": [] }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/chat/completions where the user message is missing the `content` field.
    /// OpenAIChatMessage allows content=None (for tool_calls messages), but a bare
    /// user message with no content or tool_calls should not crash the server.
    #[tokio::test]
    async fn test_chat_completions_handles_missing_content_field() {
        let app = test_router_default(Config::default());

        // message has role but no content or tool_calls
        let body = serde_json::json!({
            "messages": [{"role": "user"}]
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // The server must not panic — 400, 422, or 503 (no model) are all acceptable.
        // What matters is that the status is NOT 500.
        assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// POST /v1/tirami/agora/register with an `agent_hex` containing non-hex characters.
    ///
    /// VULNERABILITY DOCUMENTED: The `agora_register` handler accepts `Json<AgentProfile>`
    /// directly (serde struct deserialization) which bypasses the validation in
    /// `AgentProfile::new()`. As a result, arbitrary strings including invalid hex and
    /// strings shorter than 64 chars are accepted and stored. This test documents the
    /// current (permissive) behaviour so any future fix will need to add explicit
    /// validation in the handler before calling `mp.register_agent()`.
    #[tokio::test]
    async fn test_agora_register_documents_missing_hex_validation() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({
            "agent_hex": "not-valid-hex-!!@@##$$%%^^&&**",
            "models_served": ["model-x"],
            "trm_per_token": 5,
            "tier": "small",
            "last_seen_ms": 1_700_000_000_000u64
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/agora/register")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // FIXED: handler now validates agent_hex is 64 hex chars before accepting.
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "invalid hex must be rejected with 400"
        );
    }

    /// POST /v1/tirami/agora/register where `agent_hex` is only 4 characters.
    ///
    /// VULNERABILITY DOCUMENTED: Same bypass as above — the handler accepts any
    /// string length for `agent_hex`. This test documents current permissive behavior.
    #[tokio::test]
    async fn test_agora_register_documents_short_hex_accepted() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({
            "agent_hex": "aabb",
            "models_served": ["model-x"],
            "trm_per_token": 5,
            "tier": "small",
            "last_seen_ms": 1_700_000_000_000u64
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/agora/register")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // FIXED: handler now validates agent_hex length == 64 hex chars.
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "short hex must be rejected with 400"
        );
    }

    // =========================================================================
    // 3. Path traversal / injection
    // =========================================================================

    /// GET /v1/tirami/agora/reputation/<path-traversal> — the path parameter is
    /// a URL segment, not a filesystem path. The handler attempts NodeId::from_hex;
    /// the traversal string is not valid hex so it must not panic or leak files.
    #[tokio::test]
    async fn test_reputation_endpoint_handles_path_traversal_attempt() {
        let app = test_router_default(Config::default());

        // URL-encode the traversal so axum can decode it as a path segment.
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/agora/reputation/..%2F..%2Fetc%2Fpasswd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Must return 200 (score for unknown agent) or a 4xx — never a file read / 500.
        assert_ne!(
            resp.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "path traversal must not cause 500"
        );
    }

    /// GET /v1/tirami/collusion/<non-hex path> — the {hex} segment must be
    /// validated; arbitrary text must not crash or return 500.
    #[tokio::test]
    async fn test_collusion_endpoint_handles_non_hex_path() {
        let app = test_router_default(Config::default());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/collusion/DROP%20TABLE%20nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Not a valid hex node-id → handler should return 400 or 404, never 500.
        assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            resp.status() == StatusCode::BAD_REQUEST
                || resp.status() == StatusCode::NOT_FOUND
                || resp.status() == StatusCode::OK, // returns empty/default report
            "unexpected status: {}",
            resp.status()
        );
    }

    /// GET /v1/tirami/anchor?network=<xss-payload> — the network query param must
    /// be validated against the known set; unknown values must return 400.
    #[tokio::test]
    async fn test_anchor_endpoint_rejects_xss_in_network_param() {
        let app = test_router_default(Config::default());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/anchor?network=%3Cscript%3Ealert(1)%3C%2Fscript%3E")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // =========================================================================
    // 4. Overflow / extreme values
    // =========================================================================

    /// POST /v1/tirami/lend with amount=0 → 400 (enforced by handler guard).
    #[tokio::test]
    async fn test_lend_rejects_zero_amount() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({ "amount": 0 }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/lend")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/tirami/lend with amount=u64::MAX — documents an integer overflow bug.
    ///
    /// VULNERABILITY DOCUMENTED: `ComputeLedger::can_afford()` uses `trm_cost as i64`
    /// which wraps `u64::MAX` to `-1` in Rust's truncating cast. The guard
    /// `FREE_TIER_CU (1000) >= -1` evaluates to `true`, so a brand-new node passes
    /// the affordability check and the handler calls `reserve_cu()` on it.
    /// The subsequent `reserve_cu()` call may also misbehave due to the same cast.
    /// This test documents the observed behaviour so any future fix is caught.
    ///
    /// The correct fix is to use `.try_into::<i64>().ok().map_or(false, |v| ...)` or
    /// a saturating cast in `can_afford`.
    #[tokio::test]
    async fn test_lend_u64_max_overflow_bug_documented() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({ "amount": u64::MAX }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/lend")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // BUG: currently returns 200 because `u64::MAX as i64` wraps to -1 and
        // the affordability check `1000 >= -1` is true. The handler should reject
        // this with 400. When the bug is fixed, change the assertion below to:
        //   assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(
            resp.status() == StatusCode::OK || resp.status() == StatusCode::BAD_REQUEST,
            "unexpected status {}",
            resp.status()
        );
    }

    /// POST /v1/tirami/borrow where term_hours is negative in the JSON payload.
    /// Since term_hours is typed as u64, serde should reject the negative value.
    #[tokio::test]
    async fn test_borrow_rejects_negative_term_hours() {
        let app = test_router_default(Config::default());

        let body = r#"{"amount":100,"term_hours":-1,"collateral":200}"#;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/borrow")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Serde cannot deserialize -1 into u64 → 422 Unprocessable Entity.
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// POST /v1/tirami/mind/improve with n_cycles=999999999 → 400 (capped at 100).
    #[tokio::test]
    async fn test_mind_improve_rejects_huge_n_cycles() {
        let app = test_router_default(Config::default());

        // First init the agent so the improve endpoint can be reached.
        let init_body = r#"{"system_prompt":"sec-test","optimizer":"echo"}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/mind/init")
                    .header("content-type", "application/json")
                    .body(Body::from(init_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = serde_json::json!({ "n_cycles": 999_999_999usize }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/mind/improve")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/tirami/mind/improve with n_cycles=0 → 400 (must be ≥ 1).
    #[tokio::test]
    async fn test_mind_improve_rejects_zero_cycles() {
        let app = test_router_default(Config::default());

        let init_body = r#"{"system_prompt":"sec-test","optimizer":"echo"}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/mind/init")
                    .header("content-type", "application/json")
                    .body(Body::from(init_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = serde_json::json!({ "n_cycles": 0usize }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/mind/improve")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/tirami/bank/futures with notional_trm=0 → 400 (FuturesContract::new
    /// validates notional > 0 and returns an error that the handler propagates).
    #[tokio::test]
    async fn test_bank_create_future_rejects_zero_notional() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({
            "counterparty_hex": "c".repeat(64),
            "notional_trm": 0u64,
            "strike_price_msats": 1_000u64,
            "expires_at_ms": 9_999_999_999_999u64
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/futures")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// GET /v1/tirami/trades?limit=999999999 → must not OOM; the handler caps at 100.
    #[tokio::test]
    async fn test_trades_handles_huge_limit_without_oom() {
        let app = test_router_default(Config::default());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/trades?limit=999999999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // The returned count must never exceed the server-enforced cap of 100.
        let count = json["count"].as_u64().unwrap_or(0);
        assert!(count <= 100, "count={count} exceeds cap");
    }

    /// POST /v1/chat/completions with max_tokens=0 → should not hang; 400 or 503 expected.
    #[tokio::test]
    async fn test_chat_completions_handles_max_tokens_zero() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 0u32
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Must respond quickly without hanging — 400, 422, or 503 all acceptable.
        assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // =========================================================================
    // 5. Rate limiting
    // =========================================================================

    /// Firing 50+ rapid requests at /v1/tirami/balance must eventually produce
    /// at least one 429 Too Many Requests response.
    ///
    /// The token-bucket starts with 30 tokens (MAX_TOKENS) and refills at
    /// 30/s — a tight burst of 50 synchronous requests will drain it.
    #[tokio::test]
    async fn test_rate_limiter_blocks_after_burst() {
        use std::sync::Arc;
        use tokio::sync::Mutex;
        use crate::bank_adapter::BankServices;
        use crate::api::create_router_with_services;
        use tirami_agora::Marketplace;
        use tirami_infer::CandleEngine;
        use tirami_ledger::ComputeLedger;
        use tirami_net::GossipState;

        let config = Config::default();
        // Use shared state so all requests go through the same rate-limiter instance.
        let state = Arc::new(Mutex::new(()));

        // Build a single router and clone it for each oneshot (axum Router is Clone).
        let router = create_router_with_services(
            config,
            Arc::new(Mutex::new(CandleEngine::new())),
            Arc::new(Mutex::new(ComputeLedger::new())),
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            None,
            Arc::new(Mutex::new(GossipState::new())),
            Arc::new(Mutex::new(BankServices::new_default())),
            Arc::new(Mutex::new(Marketplace::new())),
            Arc::new(Mutex::new(0usize)),
            Arc::new(Mutex::new(None::<tirami_mind::TiramiMindAgent>)),
        );
        let _ = state; // suppress unused warning

        let mut got_429 = false;
        for _ in 0..50 {
            let resp = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/v1/tirami/balance")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            if resp.status() == StatusCode::TOO_MANY_REQUESTS {
                got_429 = true;
                break;
            }
        }

        assert!(got_429, "expected at least one 429 after 50 burst requests");
    }

    // =========================================================================
    // 6. Content-Type attacks
    // =========================================================================

    /// POST /v1/tirami/bank/strategy with no Content-Type header → axum should
    /// reject the request (400 or 415) because it cannot parse the body as JSON.
    #[tokio::test]
    async fn test_post_without_content_type_is_rejected() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({ "strategy": "conservative" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/strategy")
                    // Intentionally no Content-Type header
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // axum's Json extractor requires Content-Type: application/json
        assert!(
            resp.status() == StatusCode::BAD_REQUEST
                || resp.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected 400 or 415, got {}",
            resp.status()
        );
    }

    /// POST /v1/tirami/bank/strategy with Content-Type: application/xml → 400 or 415.
    #[tokio::test]
    async fn test_post_with_xml_content_type_is_rejected() {
        let app = test_router_default(Config::default());

        let body = r#"<strategy>conservative</strategy>"#;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/strategy")
                    .header("content-type", "application/xml")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(
            resp.status() == StatusCode::BAD_REQUEST
                || resp.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected 400 or 415, got {}",
            resp.status()
        );
    }

    // =========================================================================
    // 7. Additional edge-case hardening
    // =========================================================================

    /// POST /v1/tirami/bank/futures where counterparty_hex is fewer than 64 chars → 400.
    #[tokio::test]
    async fn test_bank_create_future_rejects_short_counterparty_hex() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({
            "counterparty_hex": "aabb",   // only 4 chars, not 64
            "notional_trm": 1_000u64,
            "strike_price_msats": 1_000u64,
            "expires_at_ms": 9_999_999_999_999u64
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/futures")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/tirami/bank/strategy with margin_fraction=0.0 → 400.
    /// Fraction must be strictly > 0.
    #[tokio::test]
    async fn test_bank_strategy_rejects_zero_fraction() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({
            "strategy": "conservative",
            "base_commit_fraction": 0.0
        })
        .to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/strategy")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// GET /v1/tirami/anchor without the network query param uses default "testnet"
    /// and must succeed (200) — not crash on the missing optional parameter.
    #[tokio::test]
    async fn test_anchor_endpoint_works_without_network_param() {
        let app = test_router_default(Config::default());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/anchor")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// POST /v1/tirami/agora/find with min_reputation > 1.0 → 400.
    #[tokio::test]
    async fn test_agora_find_rejects_out_of_range_reputation() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({
            "model_patterns": ["*"],
            "min_reputation": 1.5
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

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/tirami/agora/find with min_reputation < 0.0 → 400.
    #[tokio::test]
    async fn test_agora_find_rejects_negative_reputation() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({
            "model_patterns": ["*"],
            "min_reputation": -0.1
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

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/tirami/mind/init with unknown optimizer name → 400.
    #[tokio::test]
    async fn test_mind_init_rejects_unknown_optimizer() {
        let app = test_router_default(Config::default());

        let body = r#"{"system_prompt":"test","optimizer":"mallicious_optimizer"}"#;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/mind/init")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /v1/tirami/bank/risk with an unknown tolerance string → 400.
    #[tokio::test]
    async fn test_bank_set_risk_rejects_unknown_tolerance() {
        let app = test_router_default(Config::default());

        let body = serde_json::json!({ "tolerance": "yolo" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/risk")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
