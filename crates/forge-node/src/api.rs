use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{Request, StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{Response, Sse, sse::Event},
    routing::{get, post},
};
use forge_core::{Config, ModelManifest, NodeId, PeerCapability, PipelineTopology};
use forge_infer::{CandleEngine, InferenceEngine};
use forge_ledger::{ComputeLedger, SafetyController, SettlementStatement, TradeRecord};
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
    /// Rate limiter for economic endpoints (Issue #5).
    forge_rate_limiter: Arc<Mutex<RateLimiter>>,
    /// Safety controller — kill switch, budget policies, circuit breakers.
    safety: Arc<Mutex<SafetyController>>,
    /// Node identity for this seed (used as provider in trades).
    local_node_id: NodeId,
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

/// Token-bucket rate limiter for economic endpoints (Issue #22).
/// Refills at a fixed rate, preventing race condition bypass.
struct RateLimiter {
    tokens: f64,
    last_refill: std::time::Instant,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            tokens: Self::MAX_TOKENS,
            last_refill: std::time::Instant::now(),
        }
    }
}

impl RateLimiter {
    const MAX_TOKENS: f64 = 30.0;
    const REFILL_RATE: f64 = 30.0; // tokens per second

    fn check(&mut self) -> bool {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * Self::REFILL_RATE).min(Self::MAX_TOKENS);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
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
    // Derive local node ID from cluster or generate a deterministic one.
    let local_node_id = cluster
        .as_ref()
        .map(|c| c.local_capability().node_id.clone())
        .unwrap_or_else(|| NodeId([0u8; 32]));

    let state = AppState {
        config,
        engine,
        ledger,
        model_manifest,
        advertised_topology,
        cluster,
        auth_failures: Arc::new(Mutex::new(AuthFailureTracker::default())),
        forge_rate_limiter: Arc::new(Mutex::new(RateLimiter::default())),
        safety: Arc::new(Mutex::new(SafetyController::new())),
        local_node_id,
    };
    let api_max_request_body_bytes = state.config.api_max_request_body_bytes;

    let protected = Router::new()
        .route("/status", get(status))
        .route("/topology", get(topology))
        .route("/settlement", get(settlement))
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        // OpenAI-compatible routes
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/models", get(openai_models))
        // Forge economic routes
        .route("/v1/forge/balance", get(forge_balance))
        .route("/v1/forge/pricing", get(forge_pricing))
        .route("/v1/forge/trades", get(forge_trades))
        .route("/v1/forge/invoice", post(forge_invoice))
        .route("/v1/forge/network", get(forge_network))
        .route("/v1/forge/providers", get(forge_providers))
        .route("/v1/forge/safety", get(forge_safety_status))
        .route("/v1/forge/kill", post(forge_kill_switch))
        .route("/v1/forge/policy", post(forge_set_policy))
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

// ---------------------------------------------------------------------------
// Legacy Forge types (kept for backward compatibility)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// OpenAI-compatible types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct OpenAIChatRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub messages: Vec<OpenAIChatMessage>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stream: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct OpenAIChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: OpenAIUsage,
    /// Forge-specific extension: compute cost information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_forge: Option<ForgeUsageExt>,
}

#[derive(Debug, Serialize)]
pub struct OpenAIChoice {
    pub index: u32,
    pub message: OpenAIChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize)]
pub struct ForgeUsageExt {
    pub cu_cost: u64,
    pub effective_balance: i64,
}

/// SSE chunk for streaming completions.
#[derive(Debug, Serialize)]
struct OpenAIStreamChunk {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Serialize)]
struct OpenAIStreamChoice {
    index: u32,
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAIStreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

// ---------------------------------------------------------------------------
// Forge economic API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ForgeBalanceResponse {
    pub node_id: String,
    pub contributed: u64,
    pub consumed: u64,
    pub reserved: u64,
    pub net_balance: i64,
    pub effective_balance: i64,
    pub reputation: f64,
}

#[derive(Debug, Serialize)]
pub struct ForgePricingResponse {
    pub cu_per_token: f64,
    pub supply_factor: f64,
    pub demand_factor: f64,
    pub estimated_cost_100_tokens: u64,
    pub estimated_cost_1000_tokens: u64,
}

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_chat_request(state: &AppState, req: &ChatRequest) -> Result<(), (StatusCode, String)> {
    state
        .config
        .validate_inference_request(&req.prompt, req.max_tokens, req.temperature, Some(0.9))
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
}

/// Convert OpenAI messages array to a prompt string.
fn messages_to_prompt(messages: &[OpenAIChatMessage]) -> String {
    let mut prompt = String::new();
    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                prompt.push_str(&msg.content);
                prompt.push('\n');
            }
            "user" => {
                prompt.push_str("User: ");
                prompt.push_str(&msg.content);
                prompt.push('\n');
            }
            "assistant" => {
                prompt.push_str("Assistant: ");
                prompt.push_str(&msg.content);
                prompt.push('\n');
            }
            _ => {
                prompt.push_str(&msg.content);
                prompt.push('\n');
            }
        }
    }
    prompt.push_str("Assistant: ");
    prompt
}

/// Generate a unique request ID.
fn gen_request_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let id: u64 = rng.r#gen();
    format!("chatcmpl-{:016x}", id)
}

/// Get model name from manifest or fallback.
async fn model_name(manifest: &ModelState) -> String {
    manifest
        .lock()
        .await
        .as_ref()
        .map(|m| m.id.0.clone())
        .unwrap_or_else(|| "forge-model".to_string())
}

/// Record a trade in the ledger after inference.
async fn record_api_trade(
    ledger: &LedgerState,
    provider: &NodeId,
    tokens: u32,
    model_id: &str,
) -> u64 {
    let mut ledger = ledger.lock().await;
    let cu_cost = ledger.estimate_cost(tokens as u64, 1, 1);
    let trade = TradeRecord {
        provider: provider.clone(),
        consumer: NodeId([0u8; 32]), // API caller (anonymous local)
        cu_amount: cu_cost,
        tokens_processed: tokens as u64,
        timestamp: now_millis(),
        model_id: model_id.to_string(),
    };
    ledger.execute_trade(&trade);
    cu_cost
}

// ---------------------------------------------------------------------------
// Legacy Forge endpoints
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// OpenAI-compatible endpoints
// ---------------------------------------------------------------------------

/// POST /v1/chat/completions — OpenAI-compatible chat completions.
async fn openai_chat_completions(
    State(state): State<AppState>,
    Json(req): Json<OpenAIChatRequest>,
) -> Result<Response, (StatusCode, String)> {
    use axum::response::IntoResponse;

    if req.messages.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": {"message": "messages must not be empty", "type": "invalid_request_error"}
            })
            .to_string(),
        ));
    }

    let prompt = messages_to_prompt(&req.messages);
    let max_tokens = req.max_tokens.unwrap_or(default_max_tokens());
    let temperature = req.temperature.unwrap_or(default_temperature());
    let top_p = req.top_p.map(|v| v as f32);

    // Validate request parameters
    state
        .config
        .validate_inference_request(&prompt, max_tokens, temperature, top_p)
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "error": {"message": err.to_string(), "type": "invalid_request_error"}
                })
                .to_string(),
            )
        })?;

    let model = model_name(&state.model_manifest).await;
    let stream = req.stream.unwrap_or(false);

    if stream {
        openai_stream_response(state, prompt, max_tokens, temperature, top_p, model).await
    } else {
        openai_sync_response(state, prompt, max_tokens, temperature, top_p, model)
            .await
            .map(|json| json.into_response())
    }
}

/// Non-streaming response.
async fn openai_sync_response(
    state: AppState,
    prompt: String,
    max_tokens: u32,
    temperature: f32,
    top_p: Option<f32>,
    model: String,
) -> Result<Json<OpenAIChatResponse>, (StatusCode, String)> {
    let mut engine = state.engine.lock().await;
    if !engine.is_loaded() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "error": {"message": "model not loaded", "type": "server_error"}
            })
            .to_string(),
        ));
    }

    // Estimate prompt tokens (rough: chars / 4)
    let prompt_tokens = (prompt.len() / 4).max(1) as u32;

    let tokens = engine
        .generate(
            &prompt,
            max_tokens,
            temperature,
            top_p.map(|v| v as f64),
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "error": {"message": e.to_string(), "type": "server_error"}
                })
                .to_string(),
            )
        })?;

    drop(engine);

    let completion_tokens = tokens.len() as u32;
    let text = tokens.join("");

    // Record trade in ledger
    let cu_cost =
        record_api_trade(&state.ledger, &state.local_node_id, completion_tokens, &model).await;
    let effective_balance = state.ledger.lock().await.effective_balance(&state.local_node_id);

    Ok(Json(OpenAIChatResponse {
        id: gen_request_id(),
        object: "chat.completion".to_string(),
        created: now_secs(),
        model,
        choices: vec![OpenAIChoice {
            index: 0,
            message: OpenAIChatMessage {
                role: "assistant".to_string(),
                content: text,
            },
            finish_reason: "stop".to_string(),
        }],
        usage: OpenAIUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
        x_forge: Some(ForgeUsageExt {
            cu_cost,
            effective_balance,
        }),
    }))
}

/// Streaming SSE response in OpenAI format.
async fn openai_stream_response(
    state: AppState,
    prompt: String,
    max_tokens: u32,
    temperature: f32,
    top_p: Option<f32>,
    model: String,
) -> Result<Response, (StatusCode, String)> {
    use axum::response::IntoResponse;

    let mut engine = state.engine.lock().await;
    if !engine.is_loaded() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "error": {"message": "model not loaded", "type": "server_error"}
            })
            .to_string(),
        ));
    }

    let tokens = engine
        .generate(
            &prompt,
            max_tokens,
            temperature,
            top_p.map(|v| v as f64),
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "error": {"message": e.to_string(), "type": "server_error"}
                })
                .to_string(),
            )
        })?;

    drop(engine);

    let request_id = gen_request_id();
    let created = now_secs();
    let completion_count = tokens.len() as u32;
    let model_clone = model.clone();

    // Build SSE events: one per token, then a final [DONE]
    let mut events: Vec<Result<Event, std::convert::Infallible>> = Vec::new();

    // First chunk with role
    events.push(Ok(Event::default().data(
        serde_json::to_string(&OpenAIStreamChunk {
            id: request_id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model_clone.clone(),
            choices: vec![OpenAIStreamChoice {
                index: 0,
                delta: OpenAIStreamDelta {
                    role: Some("assistant".to_string()),
                    content: None,
                },
                finish_reason: None,
            }],
        })
        .unwrap_or_default(),
    )));

    // Content chunks
    for token in &tokens {
        events.push(Ok(Event::default().data(
            serde_json::to_string(&OpenAIStreamChunk {
                id: request_id.clone(),
                object: "chat.completion.chunk".to_string(),
                created,
                model: model_clone.clone(),
                choices: vec![OpenAIStreamChoice {
                    index: 0,
                    delta: OpenAIStreamDelta {
                        role: None,
                        content: Some(token.clone()),
                    },
                    finish_reason: None,
                }],
            })
            .unwrap_or_default(),
        )));
    }

    // Final chunk with finish_reason
    events.push(Ok(Event::default().data(
        serde_json::to_string(&OpenAIStreamChunk {
            id: request_id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model_clone.clone(),
            choices: vec![OpenAIStreamChoice {
                index: 0,
                delta: OpenAIStreamDelta {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
        })
        .unwrap_or_default(),
    )));

    // [DONE] marker
    events.push(Ok(Event::default().data("[DONE]")));

    // Record trade
    let ledger = state.ledger.clone();
    let provider = state.local_node_id.clone();
    tokio::spawn(async move {
        record_api_trade(&ledger, &provider, completion_count, &model).await;
    });

    let stream = tokio_stream::iter(events);
    Ok(Sse::new(stream).into_response())
}

/// GET /v1/models — list available models.
async fn openai_models(State(state): State<AppState>) -> Json<serde_json::Value> {
    let model = model_name(&state.model_manifest).await;
    let loaded = state.engine.lock().await.is_loaded();

    let mut models = Vec::new();
    if loaded {
        models.push(serde_json::json!({
            "id": model,
            "object": "model",
            "created": now_secs(),
            "owned_by": "forge",
        }));
    }

    Json(serde_json::json!({
        "object": "list",
        "data": models,
    }))
}

// ---------------------------------------------------------------------------
// Forge economic endpoints
// ---------------------------------------------------------------------------

/// Check rate limit for forge economic endpoints.
async fn check_forge_rate_limit(state: &AppState) -> Result<(), (StatusCode, String)> {
    if !state.forge_rate_limiter.lock().await.check() {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded on forge endpoints".to_string(),
        ));
    }
    Ok(())
}

/// GET /v1/forge/balance — caller's CU balance.
async fn forge_balance(
    State(state): State<AppState>,
) -> Result<Json<ForgeBalanceResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let node_id = &state.local_node_id;

    Ok(match ledger.get_balance(node_id) {
        Some(balance) => Json(ForgeBalanceResponse {
            node_id: node_id.to_hex(),
            contributed: balance.contributed,
            consumed: balance.consumed,
            reserved: balance.reserved,
            net_balance: balance.balance(),
            effective_balance: ledger.effective_balance(node_id),
            reputation: balance.reputation,
        }),
        None => Json(ForgeBalanceResponse {
            node_id: node_id.to_hex(),
            contributed: 0,
            consumed: 0,
            reserved: 0,
            net_balance: 0,
            effective_balance: ledger.effective_balance(node_id),
            reputation: 0.5,
        }),
    })
}

/// GET /v1/forge/pricing — current market price and cost estimates.
async fn forge_pricing(
    State(state): State<AppState>,
) -> Result<Json<ForgePricingResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let price = ledger.market_price();
    let cu_per_token = price.effective_cu_per_token();

    Ok(Json(ForgePricingResponse {
        cu_per_token,
        supply_factor: price.supply_factor,
        demand_factor: price.demand_factor,
        estimated_cost_100_tokens: ledger.estimate_cost(100, 1, 1),
        estimated_cost_1000_tokens: ledger.estimate_cost(1000, 1, 1),
    }))
}

/// GET /v1/forge/trades — recent trade history.
async fn forge_trades(
    State(state): State<AppState>,
    Query(params): Query<TradesQuery>,
) -> Result<Json<ForgeTradesResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let limit = params.limit.unwrap_or(20).min(100) as usize;
    let trades = ledger.recent_trades(limit);

    Ok(Json(ForgeTradesResponse {
        count: trades.len(),
        trades: trades
            .into_iter()
            .map(|t| TradeEntry {
                provider: t.provider.to_hex(),
                consumer: t.consumer.to_hex(),
                cu_amount: t.cu_amount,
                tokens_processed: t.tokens_processed,
                timestamp: t.timestamp,
                model_id: t.model_id,
            })
            .collect(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct TradesQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ForgeTradesResponse {
    pub count: usize,
    pub trades: Vec<TradeEntry>,
}

#[derive(Debug, Serialize)]
pub struct TradeEntry {
    pub provider: String,
    pub consumer: String,
    pub cu_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
}

/// GET /v1/forge/network — mesh-wide economic summary.
async fn forge_network(
    State(state): State<AppState>,
) -> Result<Json<ForgeNetworkResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let stats = ledger.network_stats();
    let merkle_root = hex::encode(ledger.compute_trade_merkle_root());

    Ok(Json(ForgeNetworkResponse {
        total_nodes: stats.total_nodes,
        total_contributed_cu: stats.total_contributed_cu,
        total_consumed_cu: stats.total_consumed_cu,
        total_trades: stats.total_trades,
        avg_reputation: stats.avg_reputation,
        merkle_root,
    }))
}

#[derive(Debug, Serialize)]
pub struct ForgeNetworkResponse {
    pub total_nodes: usize,
    pub total_contributed_cu: u64,
    pub total_consumed_cu: u64,
    pub total_trades: usize,
    pub avg_reputation: f64,
    /// SHA-256 Merkle root of the entire trade log.
    pub merkle_root: String,
}

/// GET /v1/forge/providers — list known providers with reputation and pricing (Issue #16).
/// Enables agents to compare providers and choose based on cost/quality.
async fn forge_providers(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let ranked = ledger.ranked_nodes();
    let price = ledger.market_price().effective_cu_per_token();

    let providers: Vec<serde_json::Value> = ranked
        .iter()
        .filter(|n| n.contributed > 0)
        .take(50)
        .map(|n| {
            let rep_cost = ledger.reputation_adjusted_cost(&n.node_id, 100);
            serde_json::json!({
                "node_id": n.node_id.to_hex(),
                "contributed_cu": n.contributed,
                "reputation": n.reputation,
                "cu_per_100_tokens": rep_cost,
                "base_cu_per_token": price,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "count": providers.len(),
        "providers": providers,
    })))
}

/// GET /v1/forge/safety — safety status for this node.
async fn forge_safety_status(
    State(state): State<AppState>,
) -> Result<Json<forge_ledger::SafetyStatus>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let safety = state.safety.lock().await;
    Ok(Json(safety.status(&state.local_node_id)))
}

/// POST /v1/forge/kill — activate or deactivate the kill switch.
async fn forge_kill_switch(
    State(state): State<AppState>,
    Json(req): Json<KillSwitchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut safety = state.safety.lock().await;
    if req.activate {
        safety
            .kill_switch
            .activate(&req.reason.unwrap_or_default(), &req.operator.unwrap_or_default());
        Ok(Json(serde_json::json!({
            "status": "KILL SWITCH ACTIVATED",
            "reason": safety.kill_switch.reason,
        })))
    } else {
        safety.kill_switch.deactivate();
        Ok(Json(serde_json::json!({"status": "kill switch deactivated"})))
    }
}

#[derive(Debug, Deserialize)]
struct KillSwitchRequest {
    activate: bool,
    reason: Option<String>,
    operator: Option<String>,
}

/// POST /v1/forge/policy — set budget policy for a node.
async fn forge_set_policy(
    State(state): State<AppState>,
    Json(req): Json<SetPolicyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let node_id = NodeId::from_hex(&req.node_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid node_id hex".to_string()))?;
    let policy = forge_ledger::BudgetPolicy {
        max_cu_per_hour: req.max_cu_per_hour.unwrap_or(10_000),
        max_cu_per_request: req.max_cu_per_request.unwrap_or(1_000),
        max_cu_lifetime: req.max_cu_lifetime.unwrap_or(1_000_000),
        human_approval_threshold: req.human_approval_threshold,
    };

    state.safety.lock().await.set_policy(&node_id, policy.clone());

    Ok(Json(serde_json::json!({
        "status": "policy set",
        "node_id": req.node_id,
        "policy": {
            "max_cu_per_hour": policy.max_cu_per_hour,
            "max_cu_per_request": policy.max_cu_per_request,
            "max_cu_lifetime": policy.max_cu_lifetime,
            "human_approval_threshold": policy.human_approval_threshold,
        }
    })))
}

#[derive(Debug, Deserialize)]
struct SetPolicyRequest {
    node_id: String,
    max_cu_per_hour: Option<u64>,
    max_cu_per_request: Option<u64>,
    max_cu_lifetime: Option<u64>,
    human_approval_threshold: Option<u64>,
}

/// POST /v1/forge/invoice — create a Lightning invoice from CU balance.
async fn forge_invoice(
    State(state): State<AppState>,
    Json(req): Json<InvoiceRequest>,
) -> Result<Json<InvoiceResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let effective = ledger.effective_balance(&state.local_node_id);

    if req.cu_amount == 0 {
        return Err((StatusCode::BAD_REQUEST, "cu_amount must be > 0".to_string()));
    }

    if (req.cu_amount as i64) > effective {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            format!(
                "insufficient balance: requested {} CU, available {}",
                req.cu_amount, effective
            ),
        ));
    }

    let rate = forge_lightning::payment::ExchangeRate::default();
    let amount_msats = rate.cu_to_msats(req.cu_amount);
    let amount_sats = amount_msats / 1000;

    Ok(Json(InvoiceResponse {
        cu_amount: req.cu_amount,
        amount_msats,
        amount_sats,
        msats_per_cu: rate.msats_per_cu,
        description: format!("Forge: {} CU settlement", req.cu_amount),
    }))
}

#[derive(Debug, Deserialize)]
pub struct InvoiceRequest {
    pub cu_amount: u64,
}

#[derive(Debug, Serialize)]
pub struct InvoiceResponse {
    pub cu_amount: u64,
    pub amount_msats: u64,
    pub amount_sats: u64,
    pub msats_per_cu: u64,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

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

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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

    #[tokio::test]
    async fn openai_models_returns_empty_when_no_model() {
        let config = Config::default();
        let app = test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 10_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["object"], "list");
        assert!(json["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn openai_completions_rejects_empty_messages() {
        let config = Config::default();
        let app = test_router(config);

        let body = serde_json::json!({
            "messages": []
        })
        .to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn forge_balance_returns_default_for_new_node() {
        let config = Config::default();
        let app = test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/forge/balance")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 10_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["effective_balance"], 1000); // free tier
    }

    #[tokio::test]
    async fn forge_pricing_returns_market_data() {
        let config = Config::default();
        let app = test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/forge/pricing")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 10_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["cu_per_token"].as_f64().unwrap() > 0.0);
        assert!(json["estimated_cost_100_tokens"].as_u64().is_some());
    }

    #[tokio::test]
    async fn forge_trades_returns_empty_initially() {
        let config = Config::default();
        let app = test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/forge/trades")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 10_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 0);
        assert!(json["trades"].as_array().unwrap().is_empty());
    }

    #[test]
    fn messages_to_prompt_formats_correctly() {
        let messages = vec![
            OpenAIChatMessage {
                role: "system".to_string(),
                content: "You are helpful.".to_string(),
            },
            OpenAIChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
        ];
        let prompt = messages_to_prompt(&messages);
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("User: Hello"));
        assert!(prompt.ends_with("Assistant: "));
    }
}
