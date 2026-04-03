use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{Request, StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{Response, Sse, sse::Event},
    routing::{get, post},
};
use forge_core::{Config, ModelManifest, PeerCapability, PipelineTopology};
use forge_infer::{CandleEngine, InferenceEngine};
use forge_ledger::{ComputeLedger, SettlementStatement};
use forge_net::ClusterManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

type EngineState = Arc<Mutex<CandleEngine>>;
type LedgerState = Arc<Mutex<ComputeLedger>>;
type ModelState = Arc<Mutex<Option<ModelManifest>>>;
type TopologyState = Arc<Mutex<Option<PipelineTopology>>>;

#[derive(Clone)]
struct AppState {
    config: Config,
    engine: EngineState,
    ledger: LedgerState,
    model_manifest: ModelState,
    advertised_topology: TopologyState,
    cluster: Option<Arc<ClusterManager>>,
    /// Track recent auth failures for rate limiting.
    auth_failures: Arc<Mutex<AuthFailureTracker>>,
}

/// Simple rate limiter for authentication failures.
struct AuthFailureTracker {
    count: u32,
    window_start: std::time::Instant,
}

impl Default for AuthFailureTracker {
    fn default() -> Self {
        Self {
            count: 0,
            window_start: std::time::Instant::now(),
        }
    }
}

impl AuthFailureTracker {
    const MAX_FAILURES_PER_MINUTE: u32 = 10;
    const WINDOW_DURATION: std::time::Duration = std::time::Duration::from_secs(60);

    fn record_failure(&mut self) -> bool {
        if self.window_start.elapsed() > Self::WINDOW_DURATION {
            self.count = 0;
            self.window_start = std::time::Instant::now();
        }
        self.count += 1;
        self.count <= Self::MAX_FAILURES_PER_MINUTE
    }

    fn is_blocked(&self) -> bool {
        self.window_start.elapsed() <= Self::WINDOW_DURATION
            && self.count > Self::MAX_FAILURES_PER_MINUTE
    }
}

pub fn create_router(
    config: Config,
    engine: EngineState,
    ledger: LedgerState,
    model_manifest: ModelState,
    advertised_topology: TopologyState,
    cluster: Option<Arc<ClusterManager>>,
) -> Router {
    let state = AppState {
        config,
        engine,
        ledger,
        model_manifest,
        advertised_topology,
        cluster,
        auth_failures: Arc::new(Mutex::new(AuthFailureTracker::default())),
    };
    let api_max_request_body_bytes = state.config.api_max_request_body_bytes;

    let protected = Router::new()
        .route("/status", get(status))
        .route("/topology", get(topology))
        .route("/settlement", get(settlement))
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer_auth,
        ));

    Router::new()
        .route("/health", get(health))
        .merge(protected)
        .layer(DefaultBodyLimit::max(api_max_request_body_bytes))
        .with_state(state)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub prompt: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_max_tokens() -> u32 {
    256
}

fn default_temperature() -> f32 {
    0.7
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub text: String,
    pub tokens_generated: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub model_loaded: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
    pub model_loaded: bool,
    pub market_price: forge_ledger::MarketPrice,
    pub network: forge_ledger::NetworkStats,
    pub recent_trades: Vec<forge_ledger::TradeRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopologyResponse {
    pub status: String,
    pub model: Option<ModelManifest>,
    pub local_capability: Option<PeerCapability>,
    pub connected_peers: Vec<PeerCapability>,
    pub planned_topology: Option<PipelineTopology>,
    pub advertised_topology: Option<PipelineTopology>,
}

#[derive(Debug, Deserialize)]
pub struct SettlementQuery {
    pub hours: Option<u64>,
    pub window_start: Option<u64>,
    pub window_end: Option<u64>,
    pub reference_price_per_cu: Option<f64>,
}

async fn require_bearer_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let Some(expected) = state
        .config
        .api_bearer_token
        .as_deref()
        .filter(|token| !token.is_empty())
    else {
        return Ok(next.run(request).await);
    };

    // Check if rate-limited due to too many auth failures
    if state.auth_failures.lock().await.is_blocked() {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "too many authentication failures, try again later".to_string(),
        ));
    }

    let Some(value) = request.headers().get(AUTHORIZATION) else {
        return Err((StatusCode::UNAUTHORIZED, "missing bearer token".to_string()));
    };

    let Ok(value) = value.to_str() else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "invalid authorization header".to_string(),
        ));
    };

    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "authorization header must use Bearer".to_string(),
        ));
    };

    // Constant-time comparison to prevent timing attacks
    if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
        state.auth_failures.lock().await.record_failure();
        return Err((StatusCode::UNAUTHORIZED, "invalid bearer token".to_string()));
    }

    Ok(next.run(request).await)
}

fn validate_chat_request(state: &AppState, req: &ChatRequest) -> Result<(), (StatusCode, String)> {
    state
        .config
        .validate_inference_request(&req.prompt, req.max_tokens, req.temperature, Some(0.9))
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let engine = state.engine.lock().await;
    Json(HealthResponse {
        status: "ok".to_string(),
        model_loaded: engine.is_loaded(),
    })
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let model_loaded = {
        let engine = state.engine.lock().await;
        engine.is_loaded()
    };

    let (market_price, network, recent_trades) = {
        let ledger = state.ledger.lock().await;
        (
            ledger.market_price().clone(),
            ledger.network_stats(),
            ledger.recent_trades(10),
        )
    };

    Json(StatusResponse {
        status: "ok".to_string(),
        model_loaded,
        market_price,
        network,
        recent_trades,
    })
}

async fn settlement(
    State(state): State<AppState>,
    Query(query): Query<SettlementQuery>,
) -> Result<Json<SettlementStatement>, (StatusCode, String)> {
    let window_end = query.window_end.unwrap_or_else(now_millis);
    let window_start = query
        .window_start
        .unwrap_or_else(|| window_end.saturating_sub(query.hours.unwrap_or(24) * 3_600_000));

    if window_start > window_end {
        return Err((
            StatusCode::BAD_REQUEST,
            "window_start must be <= window_end".to_string(),
        ));
    }

    let statement = state.ledger.lock().await.export_settlement_statement(
        window_start,
        window_end,
        query.reference_price_per_cu,
    );

    Ok(Json(statement))
}

async fn topology(
    State(state): State<AppState>,
) -> Result<Json<TopologyResponse>, (StatusCode, String)> {
    let model = state.model_manifest.lock().await.clone();
    let local_capability = state
        .cluster
        .as_ref()
        .map(|cluster| cluster.local_capability().clone());
    let connected_peers = match state.cluster.as_ref() {
        Some(cluster) => cluster
            .discovery()
            .peers_by_capability()
            .await
            .into_iter()
            .filter_map(|peer| peer.capability)
            .collect(),
        None => Vec::new(),
    };

    let snapshot =
        crate::topology::build_topology_snapshot(model, local_capability, connected_peers)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let advertised_topology = state.advertised_topology.lock().await.clone();

    Ok(Json(TopologyResponse {
        status: "ok".to_string(),
        model: snapshot.model,
        local_capability: snapshot.local_capability,
        connected_peers: snapshot.connected_peers,
        planned_topology: snapshot.planned_topology,
        advertised_topology,
    }))
}

async fn chat(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    use forge_infer::InferenceEngine;

    validate_chat_request(&state, &req)?;

    let mut engine = state.engine.lock().await;
    if !engine.is_loaded() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "model not loaded".to_string(),
        ));
    }

    let tokens = engine
        .generate(&req.prompt, req.max_tokens, req.temperature, None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let text = tokens.join("");
    let count = tokens.len();

    Ok(Json(ChatResponse {
        text,
        tokens_generated: count,
    }))
}

async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    (StatusCode, String),
> {
    use forge_infer::InferenceEngine;

    validate_chat_request(&state, &req)?;

    let mut engine_guard = state.engine.lock().await;
    if !engine_guard.is_loaded() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "model not loaded".to_string(),
        ));
    }

    // Generate all tokens (blocking in the lock, then stream out)
    let tokens = engine_guard
        .generate(&req.prompt, req.max_tokens, req.temperature, None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    drop(engine_guard);

    let stream = tokio_stream::iter(
        tokens
            .into_iter()
            .map(|token| Ok(Event::default().data(token))),
    );

    Ok(Sse::new(stream))
}

/// Constant-time byte comparison to prevent timing attacks on bearer tokens.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
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
    use axum::body::Body;
    use tower::util::ServiceExt;

    fn test_router(config: Config) -> Router {
        create_router(
            config,
            Arc::new(Mutex::new(CandleEngine::new())),
            Arc::new(Mutex::new(ComputeLedger::new())),
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            None,
        )
    }

    #[tokio::test]
    async fn health_is_not_protected_by_bearer_auth() {
        let mut config = Config::default();
        config.api_bearer_token = Some("secret".to_string());
        let app = test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn protected_routes_require_bearer_auth() {
        let mut config = Config::default();
        config.api_bearer_token = Some("secret".to_string());
        let app = test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/status")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn protected_routes_accept_valid_bearer_auth() {
        let mut config = Config::default();
        config.api_bearer_token = Some("secret".to_string());
        let app = test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/status")
                    .header(AUTHORIZATION, "Bearer secret")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn chat_rejects_requests_over_runtime_limits() {
        let mut config = Config::default();
        config.max_generate_tokens = 32;
        let app = test_router(config);

        let body = serde_json::to_vec(&ChatRequest {
            prompt: "hello".to_string(),
            max_tokens: 64,
            temperature: 0.7,
        })
        .expect("serialize");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn chat_rejects_request_bodies_over_limit() {
        let mut config = Config::default();
        config.api_max_request_body_bytes = 32;
        let app = test_router(config);

        let body = serde_json::json!({
            "prompt": "this body is intentionally much larger than the configured limit",
            "max_tokens": 16,
            "temperature": 0.7
        })
        .to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
