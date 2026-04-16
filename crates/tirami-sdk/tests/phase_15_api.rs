//! Phase 14-15 SDK additions: Schedule / Peers / chat_as (consumer header).
//!
//! These are deserialization smoke tests — no live node required.

use tirami_sdk::{PeerInfo, PeersResponse, Schedule, TiramiUsage};

#[test]
fn schedule_deserializes_from_api_response() {
    let json = serde_json::json!({
        "provider": "48b5c0f2d2be5040f425fb5cb3c0c20d16b159da24f0c685f862e9bcce4a817f",
        "estimated_trm_cost": 100,
        "model_id": "qwen2.5-0.5b-instruct-q4_k_m",
        "max_tokens": 100
    });
    let s: Schedule = serde_json::from_value(json).expect("deserialize");
    assert_eq!(s.estimated_trm_cost, 100);
    assert_eq!(s.max_tokens, 100);
    assert!(s.provider.starts_with("48b5c0f2"));
}

#[test]
fn peers_response_deserializes() {
    let json = serde_json::json!({
        "count": 1,
        "peers": [{
            "node_id": "48b5c0f2d2be5040f425fb5cb3c0c20d16b159da24f0c685f862e9bcce4a817f",
            "price_multiplier": 1.0,
            "available_cu": 1000,
            "models": ["qwen2.5-0.5b-instruct-q4_k_m"],
            "latency_hint_ms": 100,
            "latency_ema_ms": 500.0,
            "last_seen": 1776379712432u64,
            "audit_tier": "Unverified",
            "verified_trades": 0
        }]
    });
    let r: PeersResponse = serde_json::from_value(json).expect("deserialize");
    assert_eq!(r.count, 1);
    assert_eq!(r.peers.len(), 1);
    assert_eq!(r.peers[0].audit_tier, "Unverified");
    assert_eq!(r.peers[0].available_cu, 1000);
}

#[test]
fn peer_info_handles_multiple_models() {
    let json = serde_json::json!({
        "node_id": "aa".repeat(32),
        "price_multiplier": 0.75,
        "available_cu": 500,
        "models": ["qwen2.5-0.5b", "llama-3.2-1b"],
        "latency_hint_ms": 50,
        "latency_ema_ms": 45.5,
        "last_seen": 1u64,
        "audit_tier": "Established",
        "verified_trades": 42
    });
    let p: PeerInfo = serde_json::from_value(json).expect("deserialize");
    assert_eq!(p.models.len(), 2);
    assert!((p.price_multiplier - 0.75).abs() < f64::EPSILON);
    assert_eq!(p.verified_trades, 42);
}

#[test]
fn tirami_usage_parses_x_tirami_extension() {
    let json = serde_json::json!({
        "trm_cost": 15,
        "effective_balance": 985
    });
    let u: TiramiUsage = serde_json::from_value(json).expect("deserialize");
    assert_eq!(u.trm_cost, 15);
    assert_eq!(u.effective_balance, 985);
}

#[test]
fn schedule_rejects_missing_provider() {
    // Defensive: if a server returns a truncated body, we should surface the
    // error rather than silently treating provider as empty.
    let json = serde_json::json!({
        "estimated_trm_cost": 100,
        "model_id": "x",
        "max_tokens": 100
    });
    let r: Result<Schedule, _> = serde_json::from_value(json);
    assert!(r.is_err(), "missing `provider` must fail to deserialize");
}

#[test]
fn peers_empty_registry() {
    let json = serde_json::json!({
        "count": 0,
        "peers": []
    });
    let r: PeersResponse = serde_json::from_value(json).expect("deserialize");
    assert_eq!(r.count, 0);
    assert!(r.peers.is_empty());
}
