use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{sse::Event, Sse},
    routing::{get, post},
    Json, Router,
};
use forge_core::{ModelManifest, PeerCapability, PipelineTopology};
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
    engine: EngineState,
    ledger: LedgerState,
    model_manifest: ModelState,
    advertised_topology: TopologyState,
    cluster: Option<Arc<ClusterManager>>,
}

pub fn create_router(
    engine: EngineState,
    ledger: LedgerState,
    model_manifest: ModelState,
    advertised_topology: TopologyState,
    cluster: Option<Arc<ClusterManager>>,
) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/topology", get(topology))
        .route("/settlement", get(settlement))
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        .with_state(AppState {
            engine,
            ledger,
            model_manifest,
            advertised_topology,
            cluster,
        })
}

#[derive(Debug, Deserialize)]
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

    let statement = state
        .ledger
        .lock()
        .await
        .export_settlement_statement(window_start, window_end, query.reference_price_per_cu);

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

    let snapshot = crate::topology::build_topology_snapshot(model, local_capability, connected_peers)
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
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, (StatusCode, String)>
{
    use forge_infer::InferenceEngine;

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

    let stream = tokio_stream::iter(tokens.into_iter().map(|token| {
        Ok(Event::default().data(token))
    }));

    Ok(Sse::new(stream))
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
