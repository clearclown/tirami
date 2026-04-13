use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{Request, StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{Response, Sse, sse::Event},
    routing::{get, post},
};
use tirami_agora::Marketplace;
use tirami_core::{Config, ModelManifest, NodeId, PeerCapability, PipelineTopology};
use tirami_infer::{CandleEngine, InferenceEngine};
use tirami_ledger::{AgentNet, ComputeLedger, SafetyController, SettlementStatement, TradeRecord};
use tirami_net::gossip::broadcast_loan;
use tirami_net::{ClusterManager, GossipState};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

type EngineState = Arc<Mutex<CandleEngine>>;
type LedgerState = Arc<Mutex<ComputeLedger>>;
type ModelState = Arc<Mutex<Option<ModelManifest>>>;
type TopologyState = Arc<Mutex<Option<PipelineTopology>>>;

#[derive(Clone)]
pub(crate) struct AppState {
    pub config: Config,
    pub engine: EngineState,
    pub ledger: LedgerState,
    model_manifest: ModelState,
    advertised_topology: TopologyState,
    cluster: Option<Arc<ClusterManager>>,
    /// Shared gossip state for broadcasting loans/trades from API handlers.
    /// Same instance held by the pipeline coordinator so dedup is coherent.
    gossip: Arc<Mutex<GossipState>>,
    /// Track recent auth failures for rate limiting.
    auth_failures: Arc<Mutex<AuthFailureTracker>>,
    /// Rate limiter for economic endpoints (Issue #5).
    pub(crate) forge_rate_limiter: Arc<Mutex<RateLimiter>>,
    /// Safety controller — kill switch, budget policies, circuit breakers.
    safety: Arc<Mutex<SafetyController>>,
    /// AgentNet — social network for AI agents.
    agentnet: Arc<Mutex<AgentNet>>,
    /// Node identity for this seed (used as provider in trades).
    pub local_node_id: NodeId,
    /// forge-bank L2 services: PortfolioManager + futures book.
    pub bank: Arc<Mutex<crate::bank_adapter::BankServices>>,
    /// forge-agora L4 marketplace.
    pub marketplace: Arc<Mutex<Marketplace>>,
    /// Index into the ledger trade_log up to which we have already fed to the marketplace.
    pub agora_last_seen: Arc<Mutex<usize>>,
    /// forge-mind L3 agent (optional — None until POST /v1/tirami/mind/init).
    pub mind_agent: Arc<Mutex<Option<tirami_mind::TiramiMindAgent>>>,
    /// Phase 13 — staking pool for TRM lock-up.
    pub staking_pool: Arc<Mutex<tirami_ledger::StakingPool>>,
    /// Phase 13 — referral tracker for sponsor bonuses.
    pub referral_tracker: Arc<Mutex<tirami_ledger::ReferralTracker>>,
    /// Phase 13 — governance state for stake-weighted voting.
    pub governance: Arc<Mutex<tirami_ledger::GovernanceState>>,
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
    gossip: Arc<Mutex<GossipState>>,
) -> Router {
    create_router_with_services(
        config,
        engine,
        ledger,
        model_manifest,
        advertised_topology,
        cluster,
        gossip,
        Arc::new(Mutex::new(crate::bank_adapter::BankServices::new_default())),
        Arc::new(Mutex::new(Marketplace::new())),
        Arc::new(Mutex::new(0usize)),
        Arc::new(Mutex::new(None::<tirami_mind::TiramiMindAgent>)),
        Arc::new(Mutex::new(tirami_ledger::StakingPool::new())),
        Arc::new(Mutex::new(tirami_ledger::ReferralTracker::new())),
        Arc::new(Mutex::new(tirami_ledger::GovernanceState::new(0))),
    )
}

/// Extended constructor used by `TiramiNode::serve_api` when the caller supplies
/// pre-built L2/L4 services, and by `test_router_default` in tests.
pub fn create_router_with_services(
    config: Config,
    engine: EngineState,
    ledger: LedgerState,
    model_manifest: ModelState,
    advertised_topology: TopologyState,
    cluster: Option<Arc<ClusterManager>>,
    gossip: Arc<Mutex<GossipState>>,
    bank: Arc<Mutex<crate::bank_adapter::BankServices>>,
    marketplace: Arc<Mutex<Marketplace>>,
    agora_last_seen: Arc<Mutex<usize>>,
    mind_agent: Arc<Mutex<Option<tirami_mind::TiramiMindAgent>>>,
    staking_pool: Arc<Mutex<tirami_ledger::StakingPool>>,
    referral_tracker: Arc<Mutex<tirami_ledger::ReferralTracker>>,
    governance: Arc<Mutex<tirami_ledger::GovernanceState>>,
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
        gossip,
        auth_failures: Arc::new(Mutex::new(AuthFailureTracker::default())),
        forge_rate_limiter: Arc::new(Mutex::new(RateLimiter::default())),
        safety: Arc::new(Mutex::new(SafetyController::new())),
        agentnet: Arc::new(Mutex::new(AgentNet::new())),
        local_node_id,
        bank,
        marketplace,
        agora_last_seen,
        mind_agent,
        staking_pool,
        referral_tracker,
        governance,
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
        .route("/v1/tirami/balance", get(forge_balance))
        .route("/v1/tirami/pricing", get(forge_pricing))
        .route("/v1/tirami/trades", get(forge_trades))
        .route("/v1/tirami/invoice", post(forge_invoice))
        .route("/v1/tirami/network", get(tirami_network))
        .route("/v1/tirami/providers", get(forge_providers))
        .route("/v1/tirami/safety", get(forge_safety_status))
        .route("/v1/tirami/kill", post(forge_kill_switch))
        .route("/v1/tirami/policy", post(forge_set_policy))
        // Forge lending routes (Phase 5.5 / Issue #34)
        .route("/v1/tirami/lend", post(forge_lend))
        .route("/v1/tirami/lend-to", post(forge_lend_to))
        .route("/v1/tirami/borrow", post(forge_borrow))
        .route("/v1/tirami/repay", post(forge_repay))
        .route("/v1/tirami/credit", get(forge_credit))
        .route("/v1/tirami/pool", get(forge_pool))
        .route("/v1/tirami/loans", get(forge_loans))
        // Forge routing (Phase 6 / Issue #38)
        .route("/v1/tirami/route", get(forge_route))
        // AgentNet — social network for AI agents
        .route("/v1/agentnet/feed", get(agentnet_feed))
        .route("/v1/agentnet/post", post(agentnet_post))
        .route("/v1/agentnet/profile", post(agentnet_upsert_profile))
        .route("/v1/agentnet/discover", get(agentnet_discover))
        .route("/v1/agentnet/leaderboard", get(agentnet_leaderboard))
        // forge-bank L2 routes (Phase 8 / Batch B1)
        .route("/v1/tirami/bank/portfolio", get(crate::handlers::bank::bank_portfolio))
        .route("/v1/tirami/bank/tick", post(crate::handlers::bank::bank_tick))
        .route("/v1/tirami/bank/strategy", post(crate::handlers::bank::bank_set_strategy))
        .route("/v1/tirami/bank/risk", post(crate::handlers::bank::bank_set_risk))
        .route("/v1/tirami/bank/futures", get(crate::handlers::bank::bank_list_futures))
        .route("/v1/tirami/bank/futures", post(crate::handlers::bank::bank_create_futures))
        .route("/v1/tirami/bank/risk-assessment", get(crate::handlers::bank::bank_risk_assessment))
        .route("/v1/tirami/bank/optimize", post(crate::handlers::bank::bank_optimize))
        // forge-agora L4 routes (Phase 8 / Batch B2)
        .route("/v1/tirami/agora/register", post(crate::handlers::agora::agora_register))
        .route("/v1/tirami/agora/agents", get(crate::handlers::agora::agora_list_agents))
        .route("/v1/tirami/agora/reputation/{hex}", get(crate::handlers::agora::agora_reputation))
        .route("/v1/tirami/agora/find", post(crate::handlers::agora::agora_find))
        .route("/v1/tirami/agora/stats", get(crate::handlers::agora::agora_stats))
        .route("/v1/tirami/agora/snapshot", get(crate::handlers::agora::agora_snapshot))
        .route("/v1/tirami/agora/restore", post(crate::handlers::agora::agora_restore))
        // forge-mind L3 routes (Phase 8 / Batch B3)
        .route("/v1/tirami/mind/init", post(crate::handlers::mind::mind_init))
        .route("/v1/tirami/mind/state", get(crate::handlers::mind::mind_state))
        .route("/v1/tirami/mind/improve", post(crate::handlers::mind::mind_improve))
        .route("/v1/tirami/mind/budget", post(crate::handlers::mind::mind_budget))
        .route("/v1/tirami/mind/stats", get(crate::handlers::mind::mind_stats))
        // Phase 13 — tokenomics / staking / referral (su = "pull me up" namespace)
        .route("/v1/tirami/su/supply", get(crate::handlers::tokenomics::su_supply))
        .route("/v1/tirami/su/stake", get(crate::handlers::tokenomics::su_stake_info))
        .route("/v1/tirami/su/stake", post(crate::handlers::tokenomics::su_stake))
        .route("/v1/tirami/su/unstake", post(crate::handlers::tokenomics::su_unstake))
        .route("/v1/tirami/su/refer", post(crate::handlers::tokenomics::su_refer))
        .route("/v1/tirami/su/referrals", get(crate::handlers::tokenomics::su_referrals))
        // Phase 13 — governance (stake-weighted voting)
        .route("/v1/tirami/governance/propose", post(crate::handlers::governance::governance_propose))
        .route("/v1/tirami/governance/vote", post(crate::handlers::governance::governance_vote))
        .route("/v1/tirami/governance/proposals", get(crate::handlers::governance::governance_proposals))
        .route("/v1/tirami/governance/tally/{id}", get(crate::handlers::governance::governance_tally))
        // Phase 10 P6 — Bitcoin OP_RETURN anchoring
        .route("/v1/tirami/anchor", get(crate::handlers::anchor::anchor_handler))
        // Admin: manual state persistence trigger (Phase 9)
        .route("/v1/tirami/admin/save-state", post(admin_save_state))
        // Phase 9 A3 — Reputation gossip debug endpoints
        .route("/v1/tirami/reputation-gossip-status", get(forge_reputation_gossip_status))
        // Phase 9 A5 — Collusion resistance debug endpoint
        .route("/v1/tirami/collusion/{hex}", get(forge_collusion_report))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer_auth,
        ));

    Router::new()
        .route("/health", get(health))
        // Phase 10 P5 — Prometheus /metrics endpoint (no auth — Prometheus scrapes without tokens)
        .route("/metrics", get(crate::handlers::metrics::metrics_handler))
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
    pub market_price: tirami_ledger::MarketPrice,
    pub network: tirami_ledger::NetworkStats,
    pub recent_trades: Vec<tirami_ledger::TradeRecord>,
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
    /// Top-k sampling: restrict sampling to the k most likely tokens.
    /// Consistent with llama-server / Ollama API extension.
    #[serde(default)]
    pub top_k: Option<i32>,
    #[serde(default)]
    pub stream: Option<bool>,
    /// OpenAI tools / function calling — Phase 12 A1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAITool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}

/// A tool definition passed in the request.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAITool {
    #[serde(rename = "type")]
    pub tool_type: String, // always "function" for now
    pub function: OpenAIFunction,
}

/// Function schema inside an OpenAITool.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIFunction {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the function parameters.
    pub parameters: serde_json::Value,
}

/// How the model should decide whether to call a tool.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum ToolChoice {
    /// "auto" | "none" | "required"
    Mode(String),
    /// {"type": "function", "function": {"name": "..."}}
    Named {
        #[serde(rename = "type")]
        kind: String,
        function: NamedFunctionChoice,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NamedFunctionChoice {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIChatMessage {
    pub role: String,
    /// Content is optional — null when the message contains tool_calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls made by the assistant in this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
}

/// A tool call emitted by the model in a response message.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String, // "function"
    pub function: OpenAIFunctionCall,
}

/// The name + JSON-string arguments of a specific tool call.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIFunctionCall {
    pub name: String,
    /// JSON-encoded arguments as a string (matches OpenAI spec).
    pub arguments: String,
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
    pub x_tirami: Option<TiramiUsageExt>,
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
pub struct TiramiUsageExt {
    pub trm_cost: u64,
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
    /// Tool calls emitted in this streaming chunk (sent as a single final chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

// ---------------------------------------------------------------------------
// Forge economic API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TiramiBalanceResponse {
    pub node_id: String,
    pub contributed: u64,
    pub consumed: u64,
    pub reserved: u64,
    pub net_balance: i64,
    pub effective_balance: i64,
    pub reputation: f64,
}

#[derive(Debug, Serialize)]
pub struct TiramiPricingResponse {
    pub trm_per_token: f64,
    pub supply_factor: f64,
    pub demand_factor: f64,
    /// How much purchasing power 1 CU has (grows with network adoption).
    pub cu_purchasing_power: f64,
    /// Deflation factor (decreases as network matures).
    pub deflation_factor: f64,
    pub total_trades_ever: u64,
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
        let content = msg.content.as_deref().unwrap_or("");
        match msg.role.as_str() {
            "system" => {
                prompt.push_str(content);
                prompt.push('\n');
            }
            "user" => {
                prompt.push_str("User: ");
                prompt.push_str(content);
                prompt.push('\n');
            }
            "assistant" => {
                prompt.push_str("Assistant: ");
                prompt.push_str(content);
                prompt.push('\n');
            }
            _ => {
                prompt.push_str(content);
                prompt.push('\n');
            }
        }
    }
    prompt.push_str("Assistant: ");
    prompt
}

/// Render tools list as a system-prompt preamble that any model can understand.
/// Uses a simple XML-like `<tool_call>` marker format. Future work: dispatch on
/// model_id for model-specific native tool-calling templates (Qwen, Llama-3, etc.).
fn render_tools_prompt(tools: &[OpenAITool], choice: Option<&ToolChoice>) -> String {
    let mut s = String::new();
    s.push_str("You have access to the following tools. If you need to use a tool, respond with EXACTLY this format:\n\n");
    s.push_str("<tool_call>\n{\"name\": \"tool_name\", \"arguments\": {...}}\n</tool_call>\n\n");
    s.push_str("Otherwise, respond normally.\n\n");
    s.push_str("Available tools:\n\n");
    for tool in tools {
        let params = serde_json::to_string_pretty(&tool.function.parameters).unwrap_or_default();
        s.push_str(&format!(
            "- name: {}\n  description: {}\n  parameters: {}\n\n",
            tool.function.name, tool.function.description, params
        ));
    }
    if let Some(ToolChoice::Mode(mode)) = choice {
        match mode.as_str() {
            "required" => s.push_str("You MUST use one of the tools above.\n"),
            "none" => s.push_str("Do NOT use any tools; answer directly.\n"),
            _ => {}
        }
    }
    s
}

/// Internal representation of a parsed tool call from model output.
struct ParsedToolCall {
    id: String,
    name: String,
    arguments: String,
}

/// Generate a short random hex suffix for tool call IDs.
fn short_random() -> String {
    use rand::Rng;
    let n: u32 = rand::thread_rng().r#gen();
    format!("{:08x}", n)
}

/// Extract a tool call from model output text if present.
///
/// Looks for `<tool_call>...</tool_call>` markers and parses the enclosed JSON.
/// Returns `(content_before_tool_call, Some(ParsedToolCall))` if found, or
/// `(original_text, None)` if the model responded without a tool call.
fn extract_tool_call(text: &str) -> (String, Option<ParsedToolCall>) {
    if let Some(start) = text.find("<tool_call>") {
        if let Some(end) = text.find("</tool_call>") {
            let json_str = text[start + 11..end].trim();
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(name) = parsed.get("name").and_then(|v| v.as_str()) {
                    let arguments = parsed
                        .get("arguments")
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "{}".to_string());
                    let content_before = text[..start].trim().to_string();
                    return (
                        content_before,
                        Some(ParsedToolCall {
                            id: format!("call_{}", short_random()),
                            name: name.to_string(),
                            arguments,
                        }),
                    );
                }
            }
        }
    }
    (text.to_string(), None)
}

/// Generate a unique request ID.
fn gen_request_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let id: u64 = rng.r#gen();
    format!("chatcmpl-{:016x}", id)
}

/// Get model name from manifest or fallback.
/// Returns the actual loaded model name, or "forge-no-model" if none is loaded.
async fn model_name(manifest: &ModelState) -> String {
    manifest
        .lock()
        .await
        .as_ref()
        .map(|m| m.id.0.clone())
        .unwrap_or_else(|| "forge-no-model".to_string())
}

/// Record a trade in the ledger after inference.
async fn record_api_trade(
    ledger: &LedgerState,
    provider: &NodeId,
    tokens: u32,
    model_id: &str,
) -> u64 {
    let mut ledger = ledger.lock().await;
    let trm_cost = ledger.estimate_cost(tokens as u64, 1, 1);
    let trade = TradeRecord {
        provider: provider.clone(),
        consumer: NodeId([255u8; 32]), // API caller (anonymous local)
        trm_amount: trm_cost,
        tokens_processed: tokens as u64,
        timestamp: now_millis(),
        model_id: model_id.to_string(),
    };
    ledger.execute_trade(&trade);
    trm_cost
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
    use tirami_infer::InferenceEngine;

    validate_chat_request(&state, &req)?;

    let mut engine = state.engine.lock().await;
    if !engine.is_loaded() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "model not loaded".to_string(),
        ));
    }

    let tokens = engine
        .generate(&req.prompt, req.max_tokens, req.temperature, None, None)
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
    use tirami_infer::InferenceEngine;

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
        .generate(&req.prompt, req.max_tokens, req.temperature, None, None)
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

    // If tools are provided (and tool_choice != "none"), inject a tool description
    // as a system-prompt preamble before building the final prompt string.
    let effective_messages: Vec<OpenAIChatMessage>;
    let messages_ref = if let Some(tools) = req.tools.as_deref() {
        let skip = matches!(&req.tool_choice, Some(ToolChoice::Mode(m)) if m == "none");
        if !skip && !tools.is_empty() {
            let tools_preamble = render_tools_prompt(tools, req.tool_choice.as_ref());
            // Prepend as a synthetic system message so existing prompt builder handles it.
            let mut msgs = Vec::with_capacity(req.messages.len() + 1);
            msgs.push(OpenAIChatMessage {
                role: "system".to_string(),
                content: Some(tools_preamble),
                tool_calls: None,
            });
            msgs.extend(req.messages.iter().cloned());
            effective_messages = msgs;
            effective_messages.as_slice()
        } else {
            &req.messages
        }
    } else {
        &req.messages
    };

    let prompt = messages_to_prompt(messages_ref);
    let max_tokens = req.max_tokens.unwrap_or(default_max_tokens());
    let temperature = req.temperature.unwrap_or(default_temperature());
    let top_p = req.top_p.map(|v| v as f32);
    let top_k = req.top_k;
    let has_tools = req
        .tools
        .as_ref()
        .map(|t| !t.is_empty())
        .unwrap_or(false)
        && !matches!(&req.tool_choice, Some(ToolChoice::Mode(m)) if m == "none");

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
        openai_stream_response(state, prompt, max_tokens, temperature, top_p, top_k, model, has_tools).await
    } else {
        openai_sync_response(state, prompt, max_tokens, temperature, top_p, top_k, model, has_tools)
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
    top_k: Option<i32>,
    model: String,
    has_tools: bool,
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

    // Use the engine's tokenizer for accurate prompt token count (#P11-prompt-tokens).
    let prompt_tokens = engine
        .tokenize(&prompt)
        .map(|toks| toks.len() as u32)
        .unwrap_or(1);

    let tokens = engine
        .generate(
            &prompt,
            max_tokens,
            temperature,
            top_p.map(|v| v as f64),
            top_k,
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
    let trm_cost =
        record_api_trade(&state.ledger, &state.local_node_id, completion_tokens, &model).await;
    let effective_balance = state.ledger.lock().await.effective_balance(&state.local_node_id);

    // If tools were injected, try to extract a tool call from the model output.
    let (message, finish_reason) = if has_tools {
        let (content_before, maybe_tc) = extract_tool_call(&text);
        if let Some(tc) = maybe_tc {
            let tool_call = OpenAIToolCall {
                id: tc.id,
                call_type: "function".to_string(),
                function: OpenAIFunctionCall {
                    name: tc.name,
                    arguments: tc.arguments,
                },
            };
            (
                OpenAIChatMessage {
                    role: "assistant".to_string(),
                    content: if content_before.is_empty() { None } else { Some(content_before) },
                    tool_calls: Some(vec![tool_call]),
                },
                "tool_calls".to_string(),
            )
        } else {
            (
                OpenAIChatMessage {
                    role: "assistant".to_string(),
                    content: Some(text),
                    tool_calls: None,
                },
                "stop".to_string(),
            )
        }
    } else {
        (
            OpenAIChatMessage {
                role: "assistant".to_string(),
                content: Some(text),
                tool_calls: None,
            },
            "stop".to_string(),
        )
    };

    Ok(Json(OpenAIChatResponse {
        id: gen_request_id(),
        object: "chat.completion".to_string(),
        created: now_secs(),
        model,
        choices: vec![OpenAIChoice {
            index: 0,
            message,
            finish_reason,
        }],
        usage: OpenAIUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
        x_tirami: Some(TiramiUsageExt {
            trm_cost,
            effective_balance,
        }),
    }))
}

/// Streaming SSE response in OpenAI format.
///
/// Uses `InferenceEngine::generate_streaming` so tokens are emitted to the
/// client as they are sampled, not after all generation is complete.  The SSE
/// stream has the structure:
///   1. Role chunk  — `{"delta":{"role":"assistant"}}`
///   2. N content chunks  — `{"delta":{"content":"<fragment>"}}`
///   3. Stop chunk  — `{"delta":{},"finish_reason":"stop"}`
///   4. `[DONE]` sentinel
async fn openai_stream_response(
    state: AppState,
    prompt: String,
    max_tokens: u32,
    temperature: f32,
    top_p: Option<f32>,
    top_k: Option<i32>,
    model: String,
    has_tools: bool,
) -> Result<Response, (StatusCode, String)> {
    use axum::response::IntoResponse;
    use tokio_stream::wrappers::UnboundedReceiverStream;

    {
        let engine = state.engine.lock().await;
        if !engine.is_loaded() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                serde_json::json!({
                    "error": {"message": "model not loaded", "type": "server_error"}
                })
                .to_string(),
            ));
        }
    } // release lock before blocking

    let request_id = gen_request_id();
    let created = now_secs();
    let model_for_stream = model.clone();
    let model_for_trade = model.clone();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();

    // Send the role chunk immediately (no inference needed for this).
    let role_chunk = serde_json::to_string(&OpenAIStreamChunk {
        id: request_id.clone(),
        object: "chat.completion.chunk".to_string(),
        created,
        model: model_for_stream.clone(),
        choices: vec![OpenAIStreamChoice {
            index: 0,
            delta: OpenAIStreamDelta {
                role: Some("assistant".to_string()),
                content: None,
                tool_calls: None,
            },
            finish_reason: None,
        }],
    })
    .unwrap_or_default();
    let _ = tx.send(Ok(Event::default().data(role_chunk)));

    // Clone state handles needed inside the blocking task.
    let engine_arc = state.engine.clone();
    let ledger_arc = state.ledger.clone();
    let provider_id = state.local_node_id.clone();
    let tx_content = tx.clone();
    let req_id_clone = request_id.clone();
    let model_stream_clone = model_for_stream.clone();

    // For tool-call detection, we accumulate all streamed tokens.
    // Limitation: tool_calls arrive as a single final chunk rather than incrementally.
    let accumulated_text = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let acc_for_closure = accumulated_text.clone();

    // Spawn a blocking task so the inference loop doesn't block the async executor.
    tokio::task::spawn_blocking(move || {
        // Acquire the engine lock on the blocking thread.
        let rt = tokio::runtime::Handle::current();
        let mut engine = rt.block_on(engine_arc.lock());

        let on_token = {
            let tx = tx_content.clone();
            let req_id = req_id_clone.clone();
            let model_name = model_stream_clone.clone();
            let acc = acc_for_closure.clone();
            Box::new(move |chunk: &str| -> bool {
                // Accumulate for post-generation tool-call extraction.
                if has_tools {
                    if let Ok(mut guard) = acc.lock() {
                        guard.push_str(chunk);
                    }
                }
                let event_data = serde_json::to_string(&OpenAIStreamChunk {
                    id: req_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created,
                    model: model_name.clone(),
                    choices: vec![OpenAIStreamChoice {
                        index: 0,
                        delta: OpenAIStreamDelta {
                            role: None,
                            content: Some(chunk.to_string()),
                            tool_calls: None,
                        },
                        finish_reason: None,
                    }],
                })
                .unwrap_or_default();
                tx.send(Ok(Event::default().data(event_data))).is_ok()
            })
        };

        let count = engine
            .generate_streaming(
                &prompt,
                max_tokens,
                temperature,
                top_p.map(|v| v as f64),
                top_k,
                on_token,
            )
            .unwrap_or(0);

        drop(engine); // release lock before async work

        // If tools were enabled, check accumulated text for a tool call.
        // The tool_calls chunk replaces the stop chunk when a call is found.
        let (finish_reason, tool_calls_for_chunk) = if has_tools {
            let full_text = accumulated_text.lock().map(|g| g.clone()).unwrap_or_default();
            let (_, maybe_tc) = extract_tool_call(&full_text);
            if let Some(tc) = maybe_tc {
                let tool_call = OpenAIToolCall {
                    id: tc.id,
                    call_type: "function".to_string(),
                    function: OpenAIFunctionCall {
                        name: tc.name,
                        arguments: tc.arguments,
                    },
                };
                ("tool_calls".to_string(), Some(vec![tool_call]))
            } else {
                ("stop".to_string(), None)
            }
        } else {
            ("stop".to_string(), None)
        };

        // Stop / tool_calls chunk
        let stop_data = serde_json::to_string(&OpenAIStreamChunk {
            id: req_id_clone.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model_stream_clone.clone(),
            choices: vec![OpenAIStreamChoice {
                index: 0,
                delta: OpenAIStreamDelta {
                    role: None,
                    content: None,
                    tool_calls: tool_calls_for_chunk,
                },
                finish_reason: Some(finish_reason),
            }],
        })
        .unwrap_or_default();
        let _ = tx_content.send(Ok(Event::default().data(stop_data)));

        // [DONE] sentinel
        let _ = tx_content.send(Ok(Event::default().data("[DONE]")));

        // Record trade in ledger asynchronously after streaming finishes.
        rt.spawn(async move {
            record_api_trade(&ledger_arc, &provider_id, count, &model_for_trade).await;
        });
    });

    let stream = UnboundedReceiverStream::new(rx);
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
pub(crate) async fn check_forge_rate_limit(state: &AppState) -> Result<(), (StatusCode, String)> {
    if !state.forge_rate_limiter.lock().await.check() {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded on forge endpoints".to_string(),
        ));
    }
    Ok(())
}

/// GET /v1/tirami/balance — caller's CU balance.
async fn forge_balance(
    State(state): State<AppState>,
) -> Result<Json<TiramiBalanceResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let node_id = &state.local_node_id;

    Ok(match ledger.get_balance(node_id) {
        Some(balance) => Json(TiramiBalanceResponse {
            node_id: node_id.to_hex(),
            contributed: balance.contributed,
            consumed: balance.consumed,
            reserved: balance.reserved,
            net_balance: balance.balance(),
            effective_balance: ledger.effective_balance(node_id),
            reputation: balance.reputation,
        }),
        None => Json(TiramiBalanceResponse {
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

/// GET /v1/tirami/pricing — current market price and cost estimates.
async fn forge_pricing(
    State(state): State<AppState>,
) -> Result<Json<TiramiPricingResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let price = ledger.market_price();
    let trm_per_token = price.effective_trm_per_token();

    Ok(Json(TiramiPricingResponse {
        trm_per_token,
        supply_factor: price.supply_factor,
        demand_factor: price.demand_factor,
        cu_purchasing_power: price.cu_purchasing_power(),
        deflation_factor: price.deflation_factor(),
        total_trades_ever: price.total_trades_ever,
        estimated_cost_100_tokens: ledger.estimate_cost(100, 1, 1),
        estimated_cost_1000_tokens: ledger.estimate_cost(1000, 1, 1),
    }))
}

/// GET /v1/tirami/trades — recent trade history.
async fn forge_trades(
    State(state): State<AppState>,
    Query(params): Query<TradesQuery>,
) -> Result<Json<TiramiTradesResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let limit = params.limit.unwrap_or(20).min(100) as usize;
    let trades = ledger.recent_trades(limit);

    Ok(Json(TiramiTradesResponse {
        count: trades.len(),
        trades: trades
            .into_iter()
            .map(|t| TradeEntry {
                provider: t.provider.to_hex(),
                consumer: t.consumer.to_hex(),
                trm_amount: t.trm_amount,
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
pub struct TiramiTradesResponse {
    pub count: usize,
    pub trades: Vec<TradeEntry>,
}

#[derive(Debug, Serialize)]
pub struct TradeEntry {
    pub provider: String,
    pub consumer: String,
    pub trm_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
}

/// GET /v1/tirami/network — mesh-wide economic summary.
async fn tirami_network(
    State(state): State<AppState>,
) -> Result<Json<TiramiNetworkResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let stats = ledger.network_stats();
    let merkle_root = hex::encode(ledger.compute_trade_merkle_root());

    Ok(Json(TiramiNetworkResponse {
        total_nodes: stats.total_nodes,
        total_contributed_cu: stats.total_contributed_cu,
        total_consumed_cu: stats.total_consumed_cu,
        total_trades: stats.total_trades,
        avg_reputation: stats.avg_reputation,
        merkle_root,
    }))
}

#[derive(Debug, Serialize)]
pub struct TiramiNetworkResponse {
    pub total_nodes: usize,
    pub total_contributed_cu: u64,
    pub total_consumed_cu: u64,
    pub total_trades: usize,
    pub avg_reputation: f64,
    /// SHA-256 Merkle root of the entire trade log.
    pub merkle_root: String,
}

/// GET /v1/tirami/providers — list known providers with reputation and pricing (Issue #16).
/// Enables agents to compare providers and choose based on cost/quality.
async fn forge_providers(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let ranked = ledger.ranked_nodes();
    let price = ledger.market_price().effective_trm_per_token();

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
                "base_trm_per_token": price,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "count": providers.len(),
        "providers": providers,
    })))
}

/// GET /v1/tirami/safety — safety status for this node.
async fn forge_safety_status(
    State(state): State<AppState>,
) -> Result<Json<tirami_ledger::SafetyStatus>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let safety = state.safety.lock().await;
    Ok(Json(safety.status(&state.local_node_id)))
}

/// POST /v1/tirami/kill — activate or deactivate the kill switch.
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

/// POST /v1/tirami/policy — set budget policy for a node.
async fn forge_set_policy(
    State(state): State<AppState>,
    Json(req): Json<SetPolicyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let node_id = NodeId::from_hex(&req.node_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid node_id hex".to_string()))?;
    let policy = tirami_ledger::BudgetPolicy {
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

// ---------------------------------------------------------------------------
// Forge Lending API (Phase 5.5 — Issue #34)
// ---------------------------------------------------------------------------
//
// This is an MVP wiring of the lending endpoints onto the local ledger.
// Real cross-mesh lending requires LoanProposal/LoanAccept wire messages
// gossiped via the existing trade-gossip path (Batch B2). Until then,
// borrows are recorded as self-signed loans on a freshly generated keypair
// so the API is exercisable end-to-end in a single-node test scenario.

#[derive(Debug, Deserialize)]
struct LendRequest {
    /// Amount of CU to offer to the lending pool.
    amount: u64,
    /// Maximum loan term this lender will accept (hours).
    #[serde(default = "default_max_term")]
    max_term_hours: u64,
    /// Minimum interest rate this lender will accept (per hour).
    #[serde(default)]
    min_interest_rate: f64,
}

fn default_max_term() -> u64 {
    tirami_ledger::lending::MAX_LOAN_TERM_HOURS
}

#[derive(Debug, Serialize)]
struct LendResponse {
    pool_total_trm: u64,
    pool_available_cu: u64,
    your_contribution_cu: u64,
    accepted_max_term_hours: u64,
    accepted_min_interest_rate: f64,
}

#[derive(Debug, Deserialize)]
struct BorrowRequest {
    /// Principal amount to borrow.
    amount: u64,
    /// Loan term in hours.
    term_hours: u64,
    /// Collateral to lock (must satisfy max_ltv).
    collateral: u64,
    /// Optional: specify lender (otherwise self-borrow from own pool for testing).
    #[serde(default)]
    lender: Option<String>,
}

#[derive(Debug, Serialize)]
struct BorrowResponse {
    loan_id: String,
    principal_trm: u64,
    interest_rate_per_hour: f64,
    term_hours: u64,
    due_at: u64,
    total_due_cu: u64,
}

#[derive(Debug, Deserialize)]
struct RepayRequest {
    /// Hex-encoded loan_id (64 chars).
    loan_id: String,
}

#[derive(Debug, Serialize)]
struct RepayResponse {
    loan_id: String,
    status: String,
    principal_trm: u64,
    interest_paid_cu: u64,
}

#[derive(Debug, Serialize)]
struct CreditResponse {
    node_id: String,
    score: f64,
    components: CreditComponents,
}

#[derive(Debug, Serialize)]
struct CreditComponents {
    trade: f64,
    repayment: f64,
    uptime: f64,
    age: f64,
}

#[derive(Debug, Serialize)]
struct PoolResponse {
    total_trm: u64,
    lent_cu: u64,
    available_cu: u64,
    reserve_ratio: f64,
    active_loan_count: usize,
    avg_interest_rate: f64,
    your_max_borrow_cu: u64,
    your_offered_rate: f64,
}

#[derive(Debug, Serialize)]
struct LoanSummary {
    loan_id: String,
    role: String,
    counterparty: String,
    principal_trm: u64,
    interest_rate_per_hour: f64,
    term_hours: u64,
    collateral_trm: u64,
    status: String,
    created_at: u64,
    due_at: u64,
}

#[derive(Debug, Serialize)]
struct LoansResponse {
    count: usize,
    loans: Vec<LoanSummary>,
}

#[derive(Debug, Deserialize)]
struct RouteQuery {
    model: Option<String>,
    max_cu: Option<u64>,
    #[serde(default = "default_mode")]
    mode: String,
    max_tokens: Option<u64>,
}

fn default_mode() -> String {
    "balanced".to_string()
}

#[derive(Debug, Serialize)]
struct RouteResponse {
    provider: String,
    model: String,
    estimated_cu: u64,
    provider_reputation: f64,
    score: f64,
}

/// POST /v1/tirami/lend — offer CU to the lending pool.
///
/// MVP behavior: reserves the lender's CU (so it cannot be double-spent on
/// inference) and returns the current pool snapshot. The pool counter itself
/// is grown by `create_loan` once a borrower draws against it; the lend
/// endpoint communicates intent + reserves liquidity. Real pool deposits
/// land in Batch B2 once gossiped LoanProposals are wired.
async fn forge_lend(
    State(state): State<AppState>,
    Json(req): Json<LendRequest>,
) -> Result<Json<LendResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    if req.amount == 0 {
        return Err((StatusCode::BAD_REQUEST, "amount must be > 0".into()));
    }
    let mut ledger = state.ledger.lock().await;
    let lender = state.local_node_id.clone();
    if !ledger.can_afford(&lender, req.amount) {
        return Err((
            StatusCode::BAD_REQUEST,
            "insufficient balance to contribute".into(),
        ));
    }
    if !ledger.reserve_cu(&lender, req.amount) {
        return Err((StatusCode::BAD_REQUEST, "cannot reserve CU".into()));
    }
    let status = ledger.lending_pool_status();
    Ok(Json(LendResponse {
        pool_total_trm: status.total_pool_cu,
        pool_available_cu: status.available_cu,
        your_contribution_cu: req.amount,
        accepted_max_term_hours: req.max_term_hours,
        accepted_min_interest_rate: req.min_interest_rate,
    }))
}

#[derive(Debug, Deserialize)]
struct LendToRequest {
    /// Hex-encoded borrower NodeId (64 chars).
    borrower: String,
    /// Principal in CU.
    amount: u64,
    /// Loan term in hours.
    term_hours: u64,
    /// Collateral required from borrower.
    collateral: u64,
    /// Optional: override the computed offered interest rate.
    interest_rate_per_hour: Option<f64>,
}

#[derive(Debug, Serialize)]
struct LendToResponse {
    loan_id: String,
    principal_trm: u64,
    interest_rate_per_hour: f64,
    term_hours: u64,
    status: String,
}

/// POST /v1/tirami/lend-to — lender-initiated loan proposal to a specific borrower.
///
/// This is the lender-side counterpart to `/v1/tirami/borrow`. In a full P2P
/// deployment it would send a LoanProposal over the wire and wait for a
/// LoanAccept. The current MVP falls back to a self-signed loan (same
/// caveat as `forge_borrow`) so the endpoint is exercisable end-to-end in a
/// single-node test fixture. Real P2P wiring arrives with batch B2.
async fn forge_lend_to(
    State(state): State<AppState>,
    Json(req): Json<LendToRequest>,
) -> Result<Json<LendToResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    use ed25519_dalek::{Signer, SigningKey};
    use tirami_ledger::lending::{LoanRecord, LoanStatus, SignedLoanRecord, offered_interest_rate};
    use rand::rngs::OsRng;

    if req.amount == 0 {
        return Err((StatusCode::BAD_REQUEST, "amount must be > 0".into()));
    }

    // Decode borrower NodeId
    let borrower_bytes: [u8; 32] = hex::decode(&req.borrower)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid borrower hex".to_string()))?
        .try_into()
        .map_err(|_: Vec<u8>| {
            (StatusCode::BAD_REQUEST, "borrower must be 32 bytes".to_string())
        })?;
    let borrower = NodeId(borrower_bytes);

    // MVP: generate ephemeral lender key (same caveat as forge_borrow).
    // TODO: use node's persistent signing key from state.
    let lender_key = SigningKey::generate(&mut OsRng);
    let lender_id = NodeId(lender_key.verifying_key().to_bytes());

    // Compute borrower's credit score for rate determination.
    let ledger = state.ledger.lock().await;
    let credit = ledger.compute_credit_score(&borrower);
    drop(ledger);

    let interest_rate = req
        .interest_rate_per_hour
        .unwrap_or_else(|| offered_interest_rate(credit));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut loan = LoanRecord {
        loan_id: [0u8; 32],
        lender: lender_id.clone(),
        borrower: borrower.clone(),
        principal_trm: req.amount,
        interest_rate_per_hour: interest_rate,
        term_hours: req.term_hours,
        collateral_trm: req.collateral,
        status: LoanStatus::Active,
        created_at: now,
        due_at: now + req.term_hours.saturating_mul(3_600_000),
        repaid_at: None,
    };
    loan.loan_id = loan.compute_loan_id();

    let canonical = loan.canonical_bytes();
    let lender_sig = lender_key.sign(&canonical).to_bytes().to_vec();

    // If we have a cluster/transport, the real flow would be to send a
    // LoanProposal via P2P and wait for a LoanAccept. That plumbing lives in
    // pipeline.rs and needs batch B2's wire additions. Until then, we fall
    // through to the MVP self-sign fallback below.
    if state.cluster.is_some() {
        tracing::debug!(
            borrower = %req.borrower,
            "forge_lend_to: real P2P LoanProposal path pending batch B2 wiring"
        );
    }

    // MVP fallback: self-sign both sides. This only works if borrower ==
    // lender_id (i.e. same-node test); for real cross-node loans the borrower
    // will reject the duplicate signature.
    let signed = SignedLoanRecord {
        loan: loan.clone(),
        lender_sig: lender_sig.clone(),
        borrower_sig: lender_sig,
    };

    // Best-effort local insertion. Cross-node loans will fail the credit
    // check (borrower unknown) — that's OK, the real flow is handled in
    // pipeline.rs via LoanProposal once batch B2 lands.
    let mut ledger = state.ledger.lock().await;
    match ledger.create_loan(signed.clone()) {
        Ok(()) => {
            tracing::info!(
                loan_id = %hex::encode(loan.loan_id),
                "lend-to loan created locally"
            );
        }
        Err(e) => {
            tracing::debug!("lend-to local create_loan skipped: {}", e);
        }
    }
    drop(ledger);

    // Broadcast the proposed loan so peers become aware. Uses the shared
    // GossipState held in AppState, so dedup stays coherent with the
    // pipeline coordinator's own broadcasts.
    if let Some(cluster) = state.cluster.as_ref() {
        let transport = cluster.transport_arc();
        let gossip = state.gossip.clone();
        let signed_clone = signed.clone();
        tokio::spawn(async move {
            broadcast_loan(&transport, &gossip, &signed_clone).await;
        });
    } else {
        tracing::debug!(
            loan_id = %hex::encode(signed.loan.loan_id),
            "forge_lend_to: loan broadcast skipped: cluster not initialized"
        );
    }

    Ok(Json(LendToResponse {
        loan_id: hex::encode(loan.loan_id),
        principal_trm: loan.principal_trm,
        interest_rate_per_hour: loan.interest_rate_per_hour,
        term_hours: loan.term_hours,
        status: "proposed".into(),
    }))
}

/// POST /v1/tirami/borrow — request a CU loan.
///
/// MVP: constructs a self-signed loan against a fresh keypair so the
/// dual-signature verification path inside `create_loan` succeeds. This is
/// only meaningful inside a single-node test fixture; the production path
/// is the gossiped LoanProposal/LoanAccept handshake (Batch B2).
async fn forge_borrow(
    State(state): State<AppState>,
    Json(req): Json<BorrowRequest>,
) -> Result<Json<BorrowResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    use ed25519_dalek::{Signer, SigningKey};
    use tirami_ledger::lending::{LoanRecord, LoanStatus, SignedLoanRecord, offered_interest_rate};
    use rand::rngs::OsRng;

    if req.amount == 0 {
        return Err((StatusCode::BAD_REQUEST, "amount must be > 0".into()));
    }
    let _ = req.lender; // reserved for future P2P targeting

    let mut ledger = state.ledger.lock().await;
    let credit = ledger.compute_credit_score(&state.local_node_id);
    let interest_rate = offered_interest_rate(credit);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // MVP: one-shot signing key acting as both lender and borrower so the
    // bilateral signature check inside `create_loan` is satisfied without
    // requiring the persistent node key (which lives in forge-net).
    let signing_key = SigningKey::generate(&mut OsRng);
    let signed_node_id = NodeId(signing_key.verifying_key().to_bytes());

    let mut loan = LoanRecord {
        loan_id: [0u8; 32],
        lender: signed_node_id.clone(),
        borrower: signed_node_id,
        principal_trm: req.amount,
        interest_rate_per_hour: interest_rate,
        term_hours: req.term_hours,
        collateral_trm: req.collateral,
        status: LoanStatus::Active,
        created_at: now,
        due_at: now + req.term_hours.saturating_mul(3_600_000),
        repaid_at: None,
    };
    loan.loan_id = loan.compute_loan_id();

    let canonical = loan.canonical_bytes();
    let sig = signing_key.sign(&canonical).to_bytes().to_vec();
    let signed = SignedLoanRecord {
        loan: loan.clone(),
        lender_sig: sig.clone(),
        borrower_sig: sig,
    };

    ledger
        .create_loan(signed.clone())
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("create_loan failed: {e}")))?;
    drop(ledger);

    // Broadcast the loan to peers so ledger state propagates across the mesh.
    // Shared GossipState + transport accessor make this a real send now.
    if let Some(cluster) = state.cluster.as_ref() {
        let transport = cluster.transport_arc();
        let gossip = state.gossip.clone();
        let signed_clone = signed.clone();
        tokio::spawn(async move {
            broadcast_loan(&transport, &gossip, &signed_clone).await;
        });
    } else {
        tracing::debug!(
            loan_id = %hex::encode(signed.loan.loan_id),
            "forge_borrow: loan broadcast skipped: cluster not initialized"
        );
    }

    Ok(Json(BorrowResponse {
        loan_id: hex::encode(loan.loan_id),
        principal_trm: loan.principal_trm,
        interest_rate_per_hour: loan.interest_rate_per_hour,
        term_hours: loan.term_hours,
        due_at: loan.due_at,
        total_due_cu: loan.total_due(),
    }))
}

/// POST /v1/tirami/repay — repay an outstanding loan by id.
async fn forge_repay(
    State(state): State<AppState>,
    Json(req): Json<RepayRequest>,
) -> Result<Json<RepayResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let loan_id_bytes: [u8; 32] = hex::decode(&req.loan_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid loan_id hex".to_string()))?
        .try_into()
        .map_err(|_: Vec<u8>| (StatusCode::BAD_REQUEST, "loan_id must be 32 bytes".to_string()))?;

    let mut ledger = state.ledger.lock().await;

    // Try to snapshot principal/interest from active loans owned by this
    // node before mutation. The ledger does not yet expose a global
    // loan-by-id getter, so for loans owned by other parties we report 0.
    let snapshot = ledger
        .active_loans_for(&state.local_node_id)
        .into_iter()
        .find(|s| s.loan.loan_id == loan_id_bytes);
    let (principal, interest) = match snapshot {
        Some(s) => (s.loan.principal_trm, s.loan.total_interest()),
        None => (0, 0),
    };

    ledger
        .repay_loan(&loan_id_bytes)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("repay_loan failed: {e}")))?;

    Ok(Json(RepayResponse {
        loan_id: req.loan_id,
        status: "Repaid".into(),
        principal_trm: principal,
        interest_paid_cu: interest,
    }))
}

/// GET /v1/tirami/credit — credit score and component breakdown for the local node.
async fn forge_credit(
    State(state): State<AppState>,
) -> Result<Json<CreditResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let node_id = &state.local_node_id;
    let score = ledger.compute_credit_score(node_id);
    Ok(Json(CreditResponse {
        node_id: hex::encode(node_id.0),
        score,
        components: CreditComponents {
            trade: 0.0,
            repayment: 0.0,
            uptime: 0.0,
            age: 0.0,
        },
    }))
}

/// GET /v1/tirami/pool — lending pool status + caller-specific borrowing terms.
async fn forge_pool(
    State(state): State<AppState>,
) -> Result<Json<PoolResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    use tirami_ledger::lending::{max_borrowable, offered_interest_rate};
    let ledger = state.ledger.lock().await;
    let status = ledger.lending_pool_status();
    let credit = ledger.compute_credit_score(&state.local_node_id);
    let max_borrow = max_borrowable(credit, status.available_cu);
    let rate = offered_interest_rate(credit);
    Ok(Json(PoolResponse {
        total_trm: status.total_pool_cu,
        lent_cu: status.lent_cu,
        available_cu: status.available_cu,
        reserve_ratio: status.reserve_ratio,
        active_loan_count: status.active_loan_count,
        avg_interest_rate: status.avg_interest_rate,
        your_max_borrow_cu: max_borrow,
        your_offered_rate: rate,
    }))
}

/// GET /v1/tirami/loans — active loans where the local node is lender or borrower.
async fn forge_loans(
    State(state): State<AppState>,
) -> Result<Json<LoansResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let node_id = &state.local_node_id;
    let active = ledger.active_loans_for(node_id);
    let loans: Vec<LoanSummary> = active
        .into_iter()
        .map(|signed| {
            let role = if &signed.loan.lender == node_id {
                "lender"
            } else {
                "borrower"
            };
            let counterparty = if role == "lender" {
                hex::encode(signed.loan.borrower.0)
            } else {
                hex::encode(signed.loan.lender.0)
            };
            LoanSummary {
                loan_id: hex::encode(signed.loan.loan_id),
                role: role.to_string(),
                counterparty,
                principal_trm: signed.loan.principal_trm,
                interest_rate_per_hour: signed.loan.interest_rate_per_hour,
                term_hours: signed.loan.term_hours,
                collateral_trm: signed.loan.collateral_trm,
                status: format!("{:?}", signed.loan.status),
                created_at: signed.loan.created_at,
                due_at: signed.loan.due_at,
            }
        })
        .collect();
    Ok(Json(LoansResponse {
        count: loans.len(),
        loans,
    }))
}

// ---------------------------------------------------------------------------
// Forge Routing API (Phase 6 — Issue #38)
// ---------------------------------------------------------------------------

/// GET /v1/tirami/route — pick the optimal provider for an inference request.
///
/// `mode` can be `cost`, `quality`, or `balanced` (default). The score
/// combines reputation (quality signal) with normalized reputation-adjusted
/// price (cost signal). Returns 404 if no provider satisfies `max_cu`.
async fn forge_route(
    State(state): State<AppState>,
    Query(query): Query<RouteQuery>,
) -> Result<Json<RouteResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;

    let (quality_weight, cost_weight) = match query.mode.as_str() {
        "cost" => (0.3f64, 0.7f64),
        "quality" => (0.7f64, 0.3f64),
        _ => (0.5f64, 0.5f64),
    };
    let max_tokens = query.max_tokens.unwrap_or(1_000);
    let base_cost = ledger.estimate_cost(max_tokens, 1, 1);
    let max_cu = query.max_cu.unwrap_or(u64::MAX);
    let model = query.model.unwrap_or_else(|| "default".to_string());

    let candidates = ledger.ranked_nodes();
    let best = candidates
        .into_iter()
        .filter_map(|balance| {
            let price = ledger.reputation_adjusted_cost(&balance.node_id, base_cost);
            if price > max_cu {
                return None;
            }
            let normalized_price = price as f64 / base_cost.max(1) as f64;
            let score = balance.reputation * quality_weight - normalized_price * cost_weight;
            Some((balance.node_id.clone(), balance.reputation, price, score))
        })
        .max_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));

    let (node_id, reputation, price, score) = best
        .ok_or((StatusCode::NOT_FOUND, "no eligible provider found".into()))?;

    Ok(Json(RouteResponse {
        provider: hex::encode(node_id.0),
        model,
        estimated_cu: price,
        provider_reputation: reputation,
        score,
    }))
}

// ---------------------------------------------------------------------------
// AgentNet — Social Network for AI Agents
// ---------------------------------------------------------------------------

/// GET /v1/agentnet/feed — recent posts from agents.
async fn agentnet_feed(
    State(state): State<AppState>,
    Query(params): Query<AgentNetFeedQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let net = state.agentnet.lock().await;
    let limit = params.limit.unwrap_or(20).min(100) as usize;

    let posts: Vec<serde_json::Value> = if let Some(cat) = &params.category {
        net.feed_by_category(cat, limit)
    } else {
        net.feed(limit)
    }
    .into_iter()
    .map(|p| {
        serde_json::json!({
            "id": p.id,
            "author": p.author.to_hex(),
            "category": p.category,
            "content": p.content,
            "timestamp": p.timestamp,
            "tips": p.tips,
            "endorsements": p.endorsements.len(),
        })
    })
    .collect();

    Ok(Json(serde_json::json!({
        "count": posts.len(),
        "total_agents": net.agent_count(),
        "total_posts": net.post_count(),
        "posts": posts,
    })))
}

#[derive(Debug, Deserialize)]
struct AgentNetFeedQuery {
    limit: Option<u32>,
    category: Option<String>,
}

/// POST /v1/agentnet/post — publish to the agent network.
async fn agentnet_post(
    State(state): State<AppState>,
    Json(req): Json<AgentNetPostRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    if req.content.is_empty() || req.content.len() > 1024 {
        return Err((StatusCode::BAD_REQUEST, "content must be 1-1024 chars".to_string()));
    }

    let mut net = state.agentnet.lock().await;
    let id = net.post(state.local_node_id.clone(), &req.category, &req.content);

    Ok(Json(serde_json::json!({
        "id": id,
        "status": "posted",
    })))
}

#[derive(Debug, Deserialize)]
struct AgentNetPostRequest {
    category: String,
    content: String,
}

/// POST /v1/agentnet/profile — register or update agent profile.
async fn agentnet_upsert_profile(
    State(state): State<AppState>,
    Json(req): Json<AgentNetProfileRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    let ledger = state.ledger.lock().await;
    let balance = ledger.get_balance(&state.local_node_id);
    let (reputation, total_earned, total_spent) = match balance {
        Some(b) => (b.reputation, b.contributed, b.consumed),
        None => (0.5, 0, 0),
    };
    drop(ledger);

    let profile = tirami_ledger::AgentProfile {
        node_id: state.local_node_id.clone(),
        name: req.name,
        description: req.description,
        models: req.models,
        price_per_token: req.price_per_token,
        tags: req.tags,
        updated_at: now_millis(),
        reputation,
        total_earned,
        total_spent,
    };

    state.agentnet.lock().await.upsert_profile(profile);
    Ok(Json(serde_json::json!({"status": "profile updated"})))
}

#[derive(Debug, Deserialize)]
struct AgentNetProfileRequest {
    name: String,
    description: String,
    models: Vec<String>,
    price_per_token: Option<f64>,
    tags: Vec<String>,
}

/// GET /v1/agentnet/discover — find agents by capability tag.
async fn agentnet_discover(
    State(state): State<AppState>,
    Query(params): Query<AgentNetDiscoverQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let net = state.agentnet.lock().await;

    let agents: Vec<serde_json::Value> = net
        .discover(&params.tag)
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "node_id": a.node_id.to_hex(),
                "name": a.name,
                "description": a.description,
                "models": a.models,
                "price_per_token": a.price_per_token,
                "tags": a.tags,
                "reputation": a.reputation,
                "total_earned": a.total_earned,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "query": params.tag,
        "count": agents.len(),
        "agents": agents,
    })))
}

#[derive(Debug, Deserialize)]
struct AgentNetDiscoverQuery {
    tag: String,
}

/// GET /v1/agentnet/leaderboard — top agents by reputation.
async fn agentnet_leaderboard(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let net = state.agentnet.lock().await;

    let top: Vec<serde_json::Value> = net
        .leaderboard(20)
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name,
                "reputation": a.reputation,
                "total_earned": a.total_earned,
                "total_spent": a.total_spent,
                "tags": a.tags,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "count": top.len(),
        "agents": top,
    })))
}

/// POST /v1/tirami/invoice — create a Lightning invoice from CU balance.
async fn forge_invoice(
    State(state): State<AppState>,
    Json(req): Json<InvoiceRequest>,
) -> Result<Json<InvoiceResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let effective = ledger.effective_balance(&state.local_node_id);

    if req.trm_amount == 0 {
        return Err((StatusCode::BAD_REQUEST, "trm_amount must be > 0".to_string()));
    }

    if (req.trm_amount as i64) > effective {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            format!(
                "insufficient balance: requested {} CU, available {}",
                req.trm_amount, effective
            ),
        ));
    }

    let rate = tirami_lightning::payment::ExchangeRate::default();
    let amount_msats = rate.cu_to_msats(req.trm_amount);
    let amount_sats = amount_msats / 1000;

    Ok(Json(InvoiceResponse {
        trm_amount: req.trm_amount,
        amount_msats,
        amount_sats,
        msats_per_cu: rate.msats_per_cu,
        description: format!("Forge: {} CU settlement", req.trm_amount),
    }))
}

#[derive(Debug, Deserialize)]
pub struct InvoiceRequest {
    pub trm_amount: u64,
}

#[derive(Debug, Serialize)]
pub struct InvoiceResponse {
    pub trm_amount: u64,
    pub amount_msats: u64,
    pub amount_sats: u64,
    pub msats_per_cu: u64,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Admin endpoints (Phase 9)
// ---------------------------------------------------------------------------

/// POST /v1/tirami/admin/save-state — manually trigger L2/L3/L4 state persistence.
///
/// Useful for tests, manual backups, and graceful-shutdown scripts.
// ---------------------------------------------------------------------------
// Phase 9 A3 — Reputation gossip status (debug endpoint)
// ---------------------------------------------------------------------------

/// GET /v1/tirami/reputation-gossip-status — debug endpoint showing observed
/// reputation history per node, keyed by hex node ID.
async fn forge_reputation_gossip_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let summary: serde_json::Value = ledger
        .remote_reputation
        .iter()
        .map(|(node, obs)| {
            let node_hex = node.to_hex();
            let observations: Vec<serde_json::Value> = obs
                .iter()
                .map(|o| {
                    serde_json::json!({
                        "observer": o.observer.to_hex(),
                        "reputation": o.reputation,
                        "trade_count": o.trade_count,
                        "total_trm_volume": o.total_trm_volume,
                        "timestamp_ms": o.timestamp_ms,
                    })
                })
                .collect();
            (node_hex, serde_json::Value::Array(observations))
        })
        .collect::<serde_json::Map<_, _>>()
        .into();
    Ok(Json(serde_json::json!({
        "subjects": summary,
        "total_subjects": ledger.remote_reputation.len(),
    })))
}

// ---------------------------------------------------------------------------
// Phase 9 A5 — Collusion report (debug endpoint)
// ---------------------------------------------------------------------------

/// GET /v1/tirami/collusion/:hex — returns the CollusionReport for a node.
async fn forge_collusion_report(
    State(state): State<AppState>,
    axum::extract::Path(hex): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let node_id = tirami_core::NodeId::from_hex(&hex)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid node id: {e}")))?;
    let ledger = state.ledger.lock().await;
    let now_ms = now_millis();
    let trades = ledger.recent_trades(10_000);
    drop(ledger);
    let report = tirami_ledger::CollusionDetector::analyze_node(&trades, &node_id, now_ms);
    Ok(Json(serde_json::json!({
        "subject": hex,
        "trades_in_window": report.trades_in_window,
        "unique_counterparties": report.unique_counterparties,
        "tight_cluster_score": report.tight_cluster_score,
        "volume_spike_score": report.volume_spike_score,
        "round_robin_score": report.round_robin_score,
        "trust_penalty": report.trust_penalty,
        "flags": report.flags,
    })))
}

async fn admin_save_state(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    let mut saved: Vec<&str> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    if let Some(ref path) = state.config.bank_state_path {
        let bank = state.bank.lock().await;
        match crate::state_persist::save_bank(&*bank, path) {
            Ok(()) => saved.push("bank"),
            Err(e) => errors.push(format!("bank: {e}")),
        }
    }

    if let Some(ref path) = state.config.marketplace_state_path {
        let mp = state.marketplace.lock().await;
        match crate::state_persist::save_marketplace(&*mp, path) {
            Ok(()) => saved.push("marketplace"),
            Err(e) => errors.push(format!("marketplace: {e}")),
        }
    }

    if let Some(ref path) = state.config.mind_state_path {
        let mind = state.mind_agent.lock().await;
        if let Some(agent) = mind.as_ref() {
            match crate::state_persist::save_mind(agent, path) {
                Ok(()) => saved.push("mind"),
                Err(e) => errors.push(format!("mind: {e}")),
            }
        }
    }

    Ok(Json(serde_json::json!({
        "ok": errors.is_empty(),
        "saved": saved,
        "errors": errors,
    })))
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

/// Public alias for now_millis — used by handler modules.
pub(crate) fn now_millis_pub() -> u64 {
    now_millis()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Build a test router with default BankServices, Marketplace, and no mind agent.
/// Used by handler unit tests in `handlers/bank.rs`, `handlers/agora.rs`, and `handlers/mind.rs`.
pub(crate) fn test_router_default(config: Config) -> Router {
    use crate::bank_adapter::BankServices;
    create_router_with_services(
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
        Arc::new(Mutex::new(tirami_ledger::StakingPool::new())),
        Arc::new(Mutex::new(tirami_ledger::ReferralTracker::new())),
        Arc::new(Mutex::new(tirami_ledger::GovernanceState::new(0))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::util::ServiceExt;

    fn test_router(config: Config) -> Router {
        test_router_default(config)
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
                    .uri("/v1/tirami/balance")
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
                    .uri("/v1/tirami/pricing")
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
        assert!(json["trm_per_token"].as_f64().unwrap() > 0.0);
        assert!(json["estimated_cost_100_tokens"].as_u64().is_some());
    }

    #[tokio::test]
    async fn forge_trades_returns_empty_initially() {
        let config = Config::default();
        let app = test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/tirami/trades")
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
                content: Some("You are helpful.".to_string()),
                tool_calls: None,
            },
            OpenAIChatMessage {
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_calls: None,
            },
        ];
        let prompt = messages_to_prompt(&messages);
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("User: Hello"));
        assert!(prompt.ends_with("Assistant: "));
    }

    // -------------------------------------------------------------------------
    // P11 tests: top_p / top_k deserialization (#P11-top-p, #P11-top-k)
    // -------------------------------------------------------------------------

    #[test]
    fn test_request_top_p_top_k_deserialize() {
        // Verify OpenAIChatRequest correctly parses top_p and top_k fields.
        let json = serde_json::json!({
            "messages": [{"role": "user", "content": "hi"}],
            "top_p": 0.9,
            "top_k": 40
        });
        let req: OpenAIChatRequest = serde_json::from_value(json).expect("deserialize");
        assert_eq!(req.top_p, Some(0.9));
        assert_eq!(req.top_k, Some(40));
    }

    #[test]
    fn test_request_top_p_defaults_to_none() {
        let json = serde_json::json!({
            "messages": [{"role": "user", "content": "hi"}]
        });
        let req: OpenAIChatRequest = serde_json::from_value(json).expect("deserialize");
        assert_eq!(req.top_p, None);
        assert_eq!(req.top_k, None);
    }

    // -------------------------------------------------------------------------
    // P11 test: model name in response (#P11-model-name)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_model_name_fallback_when_no_model_loaded() {
        // When no manifest is set, model_name() should return "forge-no-model"
        // (not the old "forge-model" hardcoded string).
        let manifest_state: ModelState = Arc::new(Mutex::new(None));
        let name = model_name(&manifest_state).await;
        assert_eq!(name, "forge-no-model");
    }

    // -------------------------------------------------------------------------
    // P11 test: streaming response structure (#P11-real-streaming)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_openai_completions_rejects_no_model_with_503() {
        // When no model is loaded, completions should return 503.
        // This exercises the non-streaming path without needing a real model.
        let config = Config::default();
        let app = test_router(config);

        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 10
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

        // No model loaded → SERVICE_UNAVAILABLE
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_streaming_request_returns_503_when_no_model() {
        // Streaming path should also return 503 when no model is loaded.
        let config = Config::default();
        let app = test_router(config);

        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 10,
            "stream": true
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

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // -------------------------------------------------------------------------
    // P11 test: generate_streaming default impl (#P11-real-streaming)
    // -------------------------------------------------------------------------

    #[test]
    fn test_generate_streaming_default_impl_collects_tokens() {
        use tirami_infer::InferenceEngine;
        use tirami_core::{TiramiError, LayerRange};
        use std::path::Path;

        /// Minimal mock engine that returns a fixed set of tokens from generate().
        struct MockEngine {
            tokens: Vec<String>,
        }

        impl InferenceEngine for MockEngine {
            fn load(&mut self, _: &Path, _: &Path, _: Option<LayerRange>) -> Result<(), TiramiError> {
                Ok(())
            }
            fn is_loaded(&self) -> bool { true }
            fn generate(
                &mut self,
                _prompt: &str,
                _max_tokens: u32,
                _temperature: f32,
                _top_p: Option<f64>,
                _top_k: Option<i32>,
            ) -> Result<Vec<String>, TiramiError> {
                Ok(self.tokens.clone())
            }
            fn tokenize(&self, _prompt: &str) -> Result<Vec<u32>, TiramiError> {
                Ok(vec![1, 2, 3])
            }
            fn decode(&self, _tokens: &[u32]) -> Result<String, TiramiError> {
                Ok("decoded".to_string())
            }
            fn forward_tokens(&mut self, _: &[u32], _: usize) -> Result<Vec<f32>, TiramiError> {
                Err(TiramiError::InferenceError("not impl".to_string()))
            }
            fn sample_token(&mut self, _: &[f32], _: f32, _: Option<f64>) -> Result<u32, TiramiError> {
                Err(TiramiError::InferenceError("not impl".to_string()))
            }
        }

        let mut engine = MockEngine {
            tokens: vec!["Hello".to_string(), " ".to_string(), "world".to_string()],
        };

        // Use a channel to collect tokens from the 'static closure.
        let (collect_tx, collect_rx) = std::sync::mpsc::channel::<String>();
        let count = engine
            .generate_streaming(
                "test prompt",
                10,
                0.7,
                None,
                None,
                Box::new(move |chunk: &str| { let _ = collect_tx.send(chunk.to_string()); true }),
            )
            .expect("generate_streaming");

        let collected: Vec<String> = collect_rx.try_iter().collect();
        assert_eq!(count, 3, "should report 3 tokens generated");
        assert_eq!(collected, vec!["Hello", " ", "world"]);
    }

    // -------------------------------------------------------------------------
    // P12 A1 tests: OpenAI tools / function calling
    // -------------------------------------------------------------------------

    #[test]
    fn test_render_tools_prompt_includes_name_and_description() {
        let tools = vec![OpenAITool {
            tool_type: "function".to_string(),
            function: OpenAIFunction {
                name: "get_weather".to_string(),
                description: "Get current weather for a city".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"city": {"type": "string"}},
                    "required": ["city"]
                }),
            },
        }];
        let prompt = render_tools_prompt(&tools, None);
        assert!(prompt.contains("get_weather"), "prompt must mention tool name");
        assert!(prompt.contains("Get current weather"), "prompt must include description");
        assert!(prompt.contains("<tool_call>"), "prompt must include tool_call marker");
    }

    #[test]
    fn test_render_tools_prompt_required_mode_adds_must() {
        let tools = vec![OpenAITool {
            tool_type: "function".to_string(),
            function: OpenAIFunction {
                name: "lookup".to_string(),
                description: "Look something up".to_string(),
                parameters: serde_json::json!({}),
            },
        }];
        let prompt = render_tools_prompt(&tools, Some(&ToolChoice::Mode("required".to_string())));
        assert!(prompt.contains("MUST"), "required mode should include MUST instruction");
    }

    #[test]
    fn test_extract_tool_call_from_model_output() {
        let output = r#"Let me check that. <tool_call>{"name": "get_weather", "arguments": {"city": "Tokyo"}}</tool_call>"#;
        let (content, tc) = extract_tool_call(output);
        assert!(tc.is_some(), "should find a tool call");
        let tc = tc.unwrap();
        assert_eq!(tc.name, "get_weather");
        assert!(tc.arguments.contains("Tokyo"), "arguments should include Tokyo");
        assert!(content.contains("Let me check"), "content before tool call should be returned");
        assert!(tc.id.starts_with("call_"), "id should start with call_");
    }

    #[test]
    fn test_extract_tool_call_none_when_no_marker() {
        let (text, tc) = extract_tool_call("Just a normal response without any tool calls.");
        assert!(tc.is_none(), "should return None when no tool call marker present");
        assert_eq!(text, "Just a normal response without any tool calls.");
    }

    #[test]
    fn test_extract_tool_call_handles_malformed_json() {
        let output = "<tool_call>{garbage json here</tool_call>";
        let (_, tc) = extract_tool_call(output);
        assert!(tc.is_none(), "malformed JSON should yield None, not panic");
    }

    #[test]
    fn test_openai_chat_request_accepts_tools_field() {
        let json = r#"{
            "messages": [{"role": "user", "content": "What is the weather in Tokyo?"}],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "description": "Get weather for a city",
                        "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
                    }
                }
            ],
            "tool_choice": "auto"
        }"#;
        let req: OpenAIChatRequest = serde_json::from_str(json).expect("deserialize");
        assert!(req.tools.is_some(), "tools field should be present");
        assert_eq!(req.tools.unwrap().len(), 1);
        assert!(matches!(req.tool_choice, Some(ToolChoice::Mode(ref m)) if m == "auto"));
    }

    #[test]
    fn test_openai_chat_request_tools_optional() {
        let json = r#"{"messages":[{"role":"user","content":"hi"}]}"#;
        let req: OpenAIChatRequest = serde_json::from_str(json).expect("deserialize");
        assert!(req.tools.is_none(), "tools should be None when not provided");
        assert!(req.tool_choice.is_none(), "tool_choice should be None when not provided");
    }

    #[tokio::test]
    async fn test_chat_completions_with_tools_returns_503_when_no_model() {
        // When no model is loaded, a request with tools should still return 503.
        let config = Config::default();
        let app = test_router(config);

        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "What is the weather in Tokyo?"}],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "description": "Get weather",
                        "parameters": {}
                    }
                }
            ],
            "tool_choice": "auto",
            "max_tokens": 50
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

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
