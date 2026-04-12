//! Adapter from forge-ledger trade log to forge-agora marketplace observations.

use tirami_agora::{Marketplace, ModelTier, TradeObservation};
use tirami_ledger::{ComputeLedger, TradeRecord};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Convert a single TradeRecord to a TradeObservation.
/// Tier is inferred from the model_id; defaults to Small if unknown.
pub fn observation_from_trade(trade: &TradeRecord) -> Option<TradeObservation> {
    let tier = infer_tier(&trade.model_id);
    let provider_hex = hex::encode(trade.provider.0);
    let consumer_hex = hex::encode(trade.consumer.0);

    // TradeObservation requires provider != consumer and both must be 64-char hex
    if provider_hex == consumer_hex {
        return None;
    }

    let trade_id = format!("{:x}-{:x}", trade.timestamp, trade.trm_amount);

    TradeObservation::new(
        trade_id,
        provider_hex,
        consumer_hex,
        trade.trm_amount,
        trade.tokens_processed,
        trade.model_id.clone(),
        tier,
        trade.timestamp,
    )
    .ok()
}

fn infer_tier(model_id: &str) -> ModelTier {
    let s = model_id.to_lowercase();
    if s.contains("frontier")
        || s.contains("opus")
        || s.contains("gpt-4")
        || s.contains("claude-3")
        || s.contains("claude-opus")
    {
        ModelTier::Frontier
    } else if s.contains("70b") || s.contains("large") {
        ModelTier::Large
    } else if s.contains("13b") || s.contains("medium") || s.contains("8b") {
        ModelTier::Medium
    } else {
        ModelTier::Small
    }
}

/// Lazily refresh marketplace from the ledger trade log.
/// Drains trades after `last_seen_idx` and feeds them to marketplace.observe_trade.
/// Updates `last_seen_idx` to the new tail.
pub async fn refresh_marketplace_from_ledger(
    ledger: &Arc<Mutex<ComputeLedger>>,
    marketplace: &Arc<Mutex<Marketplace>>,
    last_seen_idx: &Arc<Mutex<usize>>,
) {
    let trades_to_observe: Vec<TradeRecord> = {
        let l = ledger.lock().await;
        // recent_trades(usize::MAX) returns all trades newest-first; we need all for slicing
        let all_trades = l.recent_trades(usize::MAX);
        let total = all_trades.len();
        let mut idx = last_seen_idx.lock().await;
        if *idx >= total {
            return;
        }
        // all_trades is newest-first, so to get trades after last_seen_idx we need
        // the first (total - *idx) entries reversed (oldest to newest among new ones)
        let new_count = total - *idx;
        let new_trades: Vec<TradeRecord> = all_trades[..new_count].iter().rev().cloned().collect();
        *idx = total;
        new_trades
    };

    if !trades_to_observe.is_empty() {
        let mut mp = marketplace.lock().await;
        for trade in &trades_to_observe {
            if let Some(obs) = observation_from_trade(trade) {
                mp.observe_trade(obs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trade(provider: [u8; 32], consumer: [u8; 32], cu: u64) -> TradeRecord {
        TradeRecord {
            provider: tirami_core::NodeId(provider),
            consumer: tirami_core::NodeId(consumer),
            trm_amount: cu,
            tokens_processed: cu / 10,
            timestamp: 1_700_000_000_000,
            model_id: "test-model".to_string(),
        }
    }

    // ===========================================================================
    // DEEP SECURITY TESTS — Round 2 (agora adapter edge cases)
    // ===========================================================================

    #[test]
    fn sec_deep_observation_from_self_trade_returns_none() {
        // provider == consumer → observation_from_trade must return None.
        let same = [1u8; 32];
        let trade = make_trade(same, same, 100);
        let obs = observation_from_trade(&trade);
        assert!(
            obs.is_none(),
            "self-trade (provider == consumer) must produce None observation"
        );
    }

    #[test]
    fn sec_deep_observation_from_valid_trade_returns_some() {
        let trade = make_trade([1u8; 32], [2u8; 32], 100);
        let obs = observation_from_trade(&trade);
        assert!(obs.is_some(), "valid distinct-party trade must produce Some observation");
    }

    #[test]
    fn sec_deep_observation_from_zero_cu_trade_handled() {
        // TradeRecord with trm_amount = 0 — the adapter should not panic.
        let trade = make_trade([3u8; 32], [4u8; 32], 0);
        let result = std::panic::catch_unwind(|| observation_from_trade(&trade));
        assert!(result.is_ok(), "zero-cu trade must not cause panic in observation_from_trade");
    }

    #[test]
    fn sec_deep_infer_tier_frontier_models() {
        assert_eq!(infer_tier("claude-opus-3-5"), ModelTier::Frontier);
        assert_eq!(infer_tier("gpt-4-turbo"), ModelTier::Frontier);
        assert_eq!(infer_tier("frontier-model"), ModelTier::Frontier);
    }

    #[test]
    fn sec_deep_infer_tier_unknown_model_defaults_to_small() {
        assert_eq!(infer_tier("unknown-model-xyz"), ModelTier::Small);
        assert_eq!(infer_tier(""), ModelTier::Small);
        assert_eq!(infer_tier("   "), ModelTier::Small);
    }

    #[test]
    fn sec_deep_infer_tier_large_models() {
        assert_eq!(infer_tier("llama-70b"), ModelTier::Large);
        assert_eq!(infer_tier("model-large-v2"), ModelTier::Large);
    }

    #[test]
    fn sec_deep_infer_tier_medium_models() {
        assert_eq!(infer_tier("llama-3-8b"), ModelTier::Medium);
        assert_eq!(infer_tier("model-13b-chat"), ModelTier::Medium);
    }
}
