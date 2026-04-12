//! HTTP handlers for `/v1/tirami/bank/*` endpoints (Phase 8 / Batch B1).
//!
//! All handlers are 100% sync internally — forge-bank has no async code.
//! We call sync forge-bank methods while holding tokio Mutex guards,
//! exactly as existing handlers call ledger.execute_trade().

use axum::{Json, extract::State, http::StatusCode};
use tirami_bank::{
    BalancedStrategy, ConservativeStrategy, Decision, FuturesContract, HighYieldStrategy,
    PortfolioManager, RiskModel, RiskTolerance, StrategyKind, YieldOptimizer,
};
use serde::{Deserialize, Serialize};

use crate::api::{AppState, check_forge_rate_limit};
use crate::bank_adapter::pool_snapshot_from_ledger;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct BankPortfolioResponse {
    pub cash_trm: u64,
    pub lent_cu: u64,
    pub borrowed_cu: u64,
    pub net_exposure_cu: i64,
    pub position_count: usize,
    pub decision_history_len: usize,
}

#[derive(Debug, Deserialize)]
pub struct StrategyRequest {
    pub strategy: String,
    #[serde(default)]
    pub base_commit_fraction: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StrategyResponse {
    pub ok: bool,
    pub strategy: String,
}

#[derive(Debug, Deserialize)]
pub struct RiskToleranceRequest {
    pub tolerance: String,
}

#[derive(Debug, Serialize)]
pub struct RiskToleranceResponse {
    pub ok: bool,
    pub tolerance: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateFuturesRequest {
    pub counterparty_hex: String,
    pub notional_trm: u64,
    pub strike_price_msats: u64,
    pub expires_at_ms: u64,
    #[serde(default)]
    pub margin_fraction: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct OptimizeRequest {
    pub max_var_99_cu: u64,
}

#[derive(Debug, Serialize)]
pub struct OptimizeResponse {
    pub applied: bool,
    pub decisions: Vec<Decision>,
    pub rationale: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/tirami/bank/portfolio
pub(crate) async fn bank_portfolio(
    State(state): State<AppState>,
) -> Result<Json<BankPortfolioResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let bank = state.bank.lock().await;
    let p = &bank.portfolio.portfolio;
    Ok(Json(BankPortfolioResponse {
        cash_trm: p.cash_trm,
        lent_cu: p.total_lent(),
        borrowed_cu: p.total_borrowed(),
        net_exposure_cu: p.net_cu_exposure(),
        position_count: p.positions.len(),
        decision_history_len: bank.portfolio.decision_history.len(),
    }))
}

/// POST /v1/tirami/bank/tick — run one strategy tick against the current ledger pool
pub(crate) async fn bank_tick(
    State(state): State<AppState>,
) -> Result<Json<Vec<Decision>>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let pool = {
        let ledger = state.ledger.lock().await;
        pool_snapshot_from_ledger(&ledger, &state.local_node_id)
    };
    let mut bank = state.bank.lock().await;
    let decisions = bank.portfolio.tick(&pool);
    Ok(Json(decisions))
}

/// POST /v1/tirami/bank/strategy — hot-swap the portfolio strategy
pub(crate) async fn bank_set_strategy(
    State(state): State<AppState>,
    Json(req): Json<StrategyRequest>,
) -> Result<Json<StrategyResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let fraction = req.base_commit_fraction.unwrap_or(0.5);
    if !(fraction > 0.0 && fraction <= 1.0) {
        return Err((StatusCode::BAD_REQUEST, "base_commit_fraction must be in (0, 1]".into()));
    }
    let new_strategy: Box<dyn tirami_bank::Strategy> = match req.strategy.as_str() {
        "conservative" => Box::new(
            ConservativeStrategy::new(fraction)
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        ),
        "highyield" | "high_yield" => Box::new(
            HighYieldStrategy::new(fraction)
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        ),
        "balanced" => Box::new(BalancedStrategy::new(fraction)),
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown strategy '{}'; use conservative|highyield|balanced", other),
            ))
        }
    };
    let strategy_name = req.strategy.clone();
    let new_strategy_kind = match req.strategy.as_str() {
        "conservative" => StrategyKind::Conservative { max_commit_fraction: fraction },
        "highyield" | "high_yield" => StrategyKind::HighYield { base_commit_fraction: fraction },
        _ => StrategyKind::Balanced { threshold: fraction },
    };
    let mut bank = state.bank.lock().await;
    // Swap the strategy: preserve portfolio and risk, replace strategy
    let old_portfolio = bank.portfolio.portfolio.clone();
    let old_risk = bank.portfolio.risk.clone();
    bank.portfolio = PortfolioManager::new(old_portfolio, new_strategy, old_risk);
    bank.strategy_kind = new_strategy_kind;
    Ok(Json(StrategyResponse {
        ok: true,
        strategy: strategy_name,
    }))
}

/// POST /v1/tirami/bank/risk — set the risk tolerance
pub(crate) async fn bank_set_risk(
    State(state): State<AppState>,
    Json(req): Json<RiskToleranceRequest>,
) -> Result<Json<RiskToleranceResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let tolerance = match req.tolerance.as_str() {
        "conservative" => RiskTolerance::Conservative,
        "balanced" => RiskTolerance::Balanced,
        "aggressive" => RiskTolerance::Aggressive,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "unknown tolerance '{}'; use conservative|balanced|aggressive",
                    other
                ),
            ))
        }
    };
    let tolerance_name = req.tolerance.clone();
    let mut bank = state.bank.lock().await;
    bank.portfolio.risk = tolerance.clone();
    bank.risk = tolerance;
    Ok(Json(RiskToleranceResponse {
        ok: true,
        tolerance: tolerance_name,
    }))
}

/// GET /v1/tirami/bank/futures
pub(crate) async fn bank_list_futures(
    State(state): State<AppState>,
) -> Result<Json<Vec<FuturesContract>>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let bank = state.bank.lock().await;
    Ok(Json(bank.futures.clone()))
}

/// POST /v1/tirami/bank/futures — create a new FuturesContract
pub(crate) async fn bank_create_futures(
    State(state): State<AppState>,
    Json(req): Json<CreateFuturesRequest>,
) -> Result<Json<FuturesContract>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let local_hex = hex::encode(state.local_node_id.0);
    if req.counterparty_hex.len() != 64 {
        return Err((StatusCode::BAD_REQUEST, "counterparty_hex must be 64 chars".into()));
    }
    let margin_cu = if let Some(frac) = req.margin_fraction {
        if !(frac > 0.0 && frac <= 1.0) {
            return Err((StatusCode::BAD_REQUEST, "margin_fraction must be in (0, 1]".into()));
        }
        (req.notional_trm as f64 * frac).floor() as u64
    } else {
        (req.notional_trm as f64 * 0.10).floor() as u64
    };
    let contract_id = format!("{:x}", crate::api::now_millis_pub());
    let contract = FuturesContract::new(
        contract_id,
        local_hex,
        req.counterparty_hex.clone(),
        req.notional_trm,
        req.strike_price_msats,
        req.expires_at_ms,
        margin_cu,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let mut bank = state.bank.lock().await;
    bank.futures.push(contract.clone());
    Ok(Json(contract))
}

/// GET /v1/tirami/bank/risk-assessment
pub(crate) async fn bank_risk_assessment(
    State(state): State<AppState>,
) -> Result<Json<tirami_bank::RiskAssessment>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let bank = state.bank.lock().await;
    let assessment = RiskModel::default().assess(&bank.portfolio.portfolio);
    Ok(Json(assessment))
}

/// POST /v1/tirami/bank/optimize
pub(crate) async fn bank_optimize(
    State(state): State<AppState>,
    Json(req): Json<OptimizeRequest>,
) -> Result<Json<OptimizeResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let pool = {
        let ledger = state.ledger.lock().await;
        pool_snapshot_from_ledger(&ledger, &state.local_node_id)
    };
    // max_var_99_cu → convert to fraction: var_fraction = max_var_99_cu / portfolio_value
    // We clamp to (0, 1] so the optimizer doesn't reject on invalid input.
    let portfolio_value = {
        let bank = state.bank.lock().await;
        (bank.portfolio.portfolio.cash_trm + bank.portfolio.portfolio.total_lent()).max(1)
    };
    let max_var_fraction = (req.max_var_99_cu as f64 / portfolio_value as f64).clamp(0.001, 1.0);

    let mut bank = state.bank.lock().await;
    // Build a YieldOptimizer from the current portfolio + strategy (cloned)
    let current_portfolio = bank.portfolio.portfolio.clone();
    let current_risk = bank.portfolio.risk.clone();
    // We can't clone Box<dyn Strategy>, so build a BalancedStrategy as the trial optimizer strategy
    let optimizer_strategy = Box::new(BalancedStrategy::default());
    let mut optimizer = YieldOptimizer::new(
        current_portfolio,
        optimizer_strategy,
        RiskModel::default(),
        current_risk,
        max_var_fraction,
    );
    let result = optimizer.tick(&pool);

    // If applied, sync the real portfolio state
    if result.applied {
        bank.portfolio.portfolio = optimizer.portfolio().clone();
    }

    Ok(Json(OptimizeResponse {
        applied: result.applied,
        decisions: result.decisions,
        rationale: result.rationale,
    }))
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
    async fn test_bank_portfolio_returns_initial_state() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/bank/portfolio")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["cash_trm"].is_u64());
        assert_eq!(json["cash_trm"].as_u64().unwrap(), 10_000);
        assert_eq!(json["lent_cu"].as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_bank_tick_returns_decisions() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/tick")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn test_bank_set_strategy_conservative() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({ "strategy": "conservative" }).to_string();
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
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["strategy"], "conservative");
    }

    #[tokio::test]
    async fn test_bank_set_strategy_unknown_returns_400() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({ "strategy": "magic" }).to_string();
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

    #[tokio::test]
    async fn test_bank_set_risk_balanced() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({ "tolerance": "aggressive" }).to_string();
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
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["tolerance"], "aggressive");
    }

    #[tokio::test]
    async fn test_bank_list_futures_empty_initially() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/bank/futures")
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
    async fn test_bank_risk_assessment_returns_data() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/bank/risk-assessment")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["portfolio_value_cu"].is_u64());
    }

    #[tokio::test]
    async fn test_bank_optimize_returns_result() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({ "max_var_99_cu": 1000 }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/bank/optimize")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["applied"].is_boolean());
        assert!(json["decisions"].is_array());
    }
}
