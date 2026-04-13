//! HTTP handlers for `/v1/tirami/su/*` tokenomics endpoints (Phase 13).
//!
//! "su" = "pull me up" — the tokenomics namespace for supply, staking, and referral.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use tirami_ledger::{
    StakeDuration,
    tokenomics::{
        TOTAL_TRM_SUPPLY, current_epoch, epoch_yield_rate, supply_factor,
        FEE_ACTIVATION_THRESHOLD,
    },
};

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};

// ---------------------------------------------------------------------------
// Response / request types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SupplyResponse {
    pub total_supply: u64,
    pub total_minted: u64,
    pub supply_factor: f64,
    pub current_epoch: u32,
    pub yield_rate: f64,
    pub transaction_fee_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct StakeRequest {
    pub amount: u64,
    /// Duration string: "7d", "30d", "90d", or "365d"
    pub duration: String,
}

#[derive(Debug, Serialize)]
pub struct StakeResponse {
    pub ok: bool,
    pub multiplier: f64,
    pub unlocks_at_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct StakeInfoResponse {
    pub staked: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlocks_at_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct UnstakeResponse {
    pub ok: bool,
    pub returned: u64,
}

#[derive(Debug, Deserialize)]
pub struct ReferRequest {
    pub referred_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ReferResponse {
    pub ok: bool,
    pub referral_count: u32,
}

#[derive(Debug, Serialize)]
pub struct ReferralsResponse {
    pub count: u32,
    pub total_bonus_earned: u64,
    pub referrals: Vec<ReferralEntry>,
}

#[derive(Debug, Serialize)]
pub struct ReferralEntry {
    pub referred: String,
    pub sponsored_at_ms: u64,
    pub loan_repaid: bool,
    pub earn_threshold_met: bool,
    pub bonus_paid: bool,
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn parse_stake_duration(s: &str) -> Option<StakeDuration> {
    match s {
        "7d" => Some(StakeDuration::Days7),
        "30d" => Some(StakeDuration::Days30),
        "90d" => Some(StakeDuration::Days90),
        "365d" => Some(StakeDuration::Days365),
        _ => None,
    }
}

fn stake_duration_str(d: &StakeDuration) -> &'static str {
    match d {
        StakeDuration::Days7 => "7d",
        StakeDuration::Days30 => "30d",
        StakeDuration::Days90 => "90d",
        StakeDuration::Days365 => "365d",
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/tirami/su/supply — supply cap status
pub(crate) async fn su_supply(
    State(state): State<AppState>,
) -> Result<Json<SupplyResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let ledger = state.ledger.lock().await;
    let total_minted = ledger.total_minted;
    drop(ledger);

    let sf = supply_factor(total_minted);
    let epoch = current_epoch(total_minted);
    let yield_rate = epoch_yield_rate(total_minted);
    let fee_active = sf <= FEE_ACTIVATION_THRESHOLD;

    Ok(Json(SupplyResponse {
        total_supply: TOTAL_TRM_SUPPLY,
        total_minted,
        supply_factor: sf,
        current_epoch: epoch,
        yield_rate,
        transaction_fee_active: fee_active,
    }))
}

/// POST /v1/tirami/su/stake — create a stake
pub(crate) async fn su_stake(
    State(state): State<AppState>,
    Json(req): Json<StakeRequest>,
) -> Result<Json<StakeResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    let duration = parse_stake_duration(&req.duration)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "duration must be one of: 7d, 30d, 90d, 365d".to_string(),
            )
        })?;

    let now_ms = now_millis_pub();
    let mut pool = state.staking_pool.lock().await;
    let stake = pool
        .stake(state.local_node_id.clone(), req.amount, duration, now_ms)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(StakeResponse {
        ok: true,
        multiplier: stake.multiplier(),
        unlocks_at_ms: stake.unlocks_at_ms,
    }))
}

/// GET /v1/tirami/su/stake — get current stake info
pub(crate) async fn su_stake_info(
    State(state): State<AppState>,
) -> Result<Json<StakeInfoResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    let now_ms = now_millis_pub();
    let pool = state.staking_pool.lock().await;

    match pool.stakes.get(&state.local_node_id) {
        None => Ok(Json(StakeInfoResponse {
            staked: 0,
            duration: None,
            multiplier: None,
            locked: None,
            unlocks_at_ms: None,
        })),
        Some(s) => Ok(Json(StakeInfoResponse {
            staked: s.amount,
            duration: Some(stake_duration_str(&s.duration).to_string()),
            multiplier: Some(s.multiplier()),
            locked: Some(s.is_locked(now_ms)),
            unlocks_at_ms: Some(s.unlocks_at_ms),
        })),
    }
}

/// POST /v1/tirami/su/unstake — withdraw after lock expires
pub(crate) async fn su_unstake(
    State(state): State<AppState>,
) -> Result<Json<UnstakeResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    let now_ms = now_millis_pub();
    let mut pool = state.staking_pool.lock().await;
    let returned = pool
        .unstake(&state.local_node_id, now_ms)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(UnstakeResponse { ok: true, returned }))
}

/// POST /v1/tirami/su/refer — register a referral
pub(crate) async fn su_refer(
    State(state): State<AppState>,
    Json(req): Json<ReferRequest>,
) -> Result<Json<ReferResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    if req.referred_hex.len() != 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            "referred_hex must be 64 hex chars".to_string(),
        ));
    }
    let bytes: [u8; 32] = hex::decode(&req.referred_hex)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid referred_hex".to_string()))?
        .try_into()
        .map_err(|_: Vec<u8>| {
            (StatusCode::BAD_REQUEST, "referred_hex must be 32 bytes".to_string())
        })?;
    let referred = tirami_core::NodeId(bytes);

    let now_ms = now_millis_pub();
    let mut tracker = state.referral_tracker.lock().await;
    tracker
        .register(state.local_node_id.clone(), referred, now_ms)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let count = tracker.referral_count(&state.local_node_id);

    Ok(Json(ReferResponse {
        ok: true,
        referral_count: count,
    }))
}

/// GET /v1/tirami/su/referrals — list your referrals
pub(crate) async fn su_referrals(
    State(state): State<AppState>,
) -> Result<Json<ReferralsResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    let tracker = state.referral_tracker.lock().await;
    let count = tracker.referral_count(&state.local_node_id);
    let total_bonus_earned = tracker
        .records
        .values()
        .filter(|r| r.sponsor == state.local_node_id && r.bonus_paid)
        .count() as u64
        * tirami_ledger::referral::REFERRAL_BONUS_TRM;

    let referrals: Vec<ReferralEntry> = tracker
        .records
        .values()
        .filter(|r| r.sponsor == state.local_node_id)
        .map(|r| ReferralEntry {
            referred: hex::encode(r.referred.0),
            sponsored_at_ms: r.sponsored_at_ms,
            loan_repaid: r.loan_repaid,
            earn_threshold_met: r.earn_threshold_met,
            bonus_paid: r.bonus_paid,
        })
        .collect();

    Ok(Json(ReferralsResponse {
        count,
        total_bonus_earned,
        referrals,
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
    async fn test_su_supply_returns_initial_state() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/su/supply")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total_supply"].as_u64().unwrap(), 21_000_000_000);
        assert_eq!(json["total_minted"].as_u64().unwrap(), 0);
        assert!((json["supply_factor"].as_f64().unwrap() - 1.0).abs() < 1e-6);
        assert_eq!(json["current_epoch"].as_u64().unwrap(), 0);
        assert_eq!(json["transaction_fee_active"].as_bool().unwrap(), false);
    }

    #[tokio::test]
    async fn test_su_stake_creates_stake() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({ "amount": 10000, "duration": "90d" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/su/stake")
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
        assert!((json["multiplier"].as_f64().unwrap() - 2.0).abs() < 1e-9);
        assert!(json["unlocks_at_ms"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn test_su_stake_invalid_duration_returns_400() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({ "amount": 10000, "duration": "999d" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/su/stake")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_su_stake_info_returns_no_stake_when_empty() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/su/stake")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["staked"].as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_su_unstake_fails_when_not_staked() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/su/unstake")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_su_refer_invalid_hex_returns_400() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({ "referred_hex": "not-a-hex" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/su/refer")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_su_referrals_returns_empty_initially() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/su/referrals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["count"].as_u64().unwrap(), 0);
        assert_eq!(json["total_bonus_earned"].as_u64().unwrap(), 0);
        assert!(json["referrals"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_su_supply_yield_rate_nonzero() {
        let app = test_router_default(Config::default());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/tirami/su/supply")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // At genesis epoch 0, yield_rate = 0.001
        assert!((json["yield_rate"].as_f64().unwrap() - 0.001).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_su_stake_below_minimum_returns_400() {
        let app = test_router_default(Config::default());
        // 90d minimum is 10_000 — stake 999 should fail
        let body = serde_json::json!({ "amount": 999, "duration": "90d" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/su/stake")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_su_refer_self_returns_400() {
        // The local node ID in test_router_default is NodeId([0u8; 32])
        let app = test_router_default(Config::default());
        let zero_hex = "0".repeat(64);
        let body = serde_json::json!({ "referred_hex": zero_hex }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/su/refer")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_su_stake_7d_multiplier() {
        let app = test_router_default(Config::default());
        let body = serde_json::json!({ "amount": 100, "duration": "7d" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tirami/su/stake")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!((json["multiplier"].as_f64().unwrap() - 1.2).abs() < 1e-9);
    }
}
