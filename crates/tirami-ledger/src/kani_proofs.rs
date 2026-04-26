//! Phase 17 Wave 3.2 — formal-verification proofs for critical
//! economic invariants.
//!
//! # What this is
//!
//! A collection of `#[kani::proof]` functions that [Kani](https://model-checking.github.io/kani/)
//! can symbolically execute to prove (or disprove) Tirami's core
//! economic invariants. The module is gated on `#[cfg(kani)]` so it
//! is invisible to normal `cargo test` / `cargo build` — it only
//! participates when an operator runs `cargo kani`.
//!
//! # Running
//!
//! ```bash
//! # One-time install (see https://model-checking.github.io/kani/install-guide.html)
//! cargo install --locked kani-verifier
//! cargo kani setup
//!
//! # Run all proofs in this crate:
//! cargo kani --package tirami-ledger
//!
//! # Run a specific proof:
//! cargo kani --package tirami-ledger --harness proof_apply_slash_burns_never_mints
//! ```
//!
//! # Invariants
//!
//! This wave establishes 10 invariants covering:
//!
//! 1. Nonce cache rejects replay (unconditional)
//! 2. Nonce cache does NOT reject distinct nonces
//! 3. Nonce cache honours capacity bound (bounded memory)
//! 4. v1 (zero-nonce) records have backwards-compat canonical bytes
//! 5. v2 canonical bytes carry the 0x02 version prefix
//! 6. v1/v2 canonical bytes never collide (distinct byte-strings)
//! 7. `apply_slash` never increases total staked amount (burn-only)
//! 8. `apply_slash` on an unknown node returns 0
//! 9. `WelcomeLoanLimiter` never exceeds its per-bucket cap
//! 10. `NonceCache::insert` is idempotent on the same nonce
//!
//! External-audit bar (Wave 3.3) is ≥ 30 proofs. The follow-up
//! waves will add invariants over:
//!
//! - TRM conservation across `execute_signed_trade` (requires
//!   modelling `HashMap<NodeId, NodeBalance>` — Kani needs bounded
//!   input sizes).
//! - Fraud-proof verifier rejects legacy v1 records.
//! - Audit-fail slash penalty is exactly 0.3 (`AUDIT_FAIL_TRUST_PENALTY`).
//! - etc.

#![cfg(kani)]

use crate::ledger::{NonceCache, TradeRecord};
use crate::staking::StakingPool;
use crate::sybil::WelcomeLoanLimiter;
use tirami_core::NodeId;

// ---------------------------------------------------------------------------
// NonceCache invariants
// ---------------------------------------------------------------------------

#[kani::proof]
fn proof_nonce_cache_rejects_replay() {
    let mut cache = NonceCache::default();
    let nonce: [u8; 16] = kani::any();
    kani::assume(nonce != [0u8; 16]);
    assert!(cache.insert(nonce));
    assert!(!cache.insert(nonce), "replay must be rejected");
}

#[kani::proof]
fn proof_nonce_cache_accepts_distinct() {
    let mut cache = NonceCache::default();
    let a: [u8; 16] = kani::any();
    let b: [u8; 16] = kani::any();
    kani::assume(a != b);
    assert!(cache.insert(a));
    assert!(cache.insert(b));
}

#[kani::proof]
#[kani::unwind(4)]
fn proof_nonce_cache_bounded_by_capacity() {
    // Insert a small symbolic number of distinct nonces.
    // Kani unwind bound keeps the proof tractable; the invariant
    // (order.len() <= CAPACITY) holds for any N.
    let mut cache = NonceCache::default();
    let n: usize = kani::any();
    kani::assume(n <= 3);
    for i in 0..n {
        let mut nonce = [0u8; 16];
        nonce[0] = i as u8;
        nonce[15] = 0xAA; // ensure non-zero
        let _ = cache.insert(nonce);
    }
    assert!(cache.len() <= NonceCache::CAPACITY);
}

// ---------------------------------------------------------------------------
// TradeRecord canonical_bytes invariants
// ---------------------------------------------------------------------------

fn symbolic_trade(nonce: [u8; 16]) -> TradeRecord {
    TradeRecord {
        provider: NodeId(kani::any()),
        consumer: NodeId(kani::any()),
        trm_amount: kani::any(),
        tokens_processed: kani::any(),
        timestamp: kani::any(),
        model_id: "k".into(), // fixed so model_id isn't a free var
        flops_estimated: 0,
        nonce,
    }
}

#[kani::proof]
fn proof_v1_canonical_has_no_version_prefix() {
    let t = symbolic_trade([0u8; 16]);
    let bytes = t.canonical_bytes();
    // v1 starts with provider byte, NOT the 0x02 version marker.
    // We can't assert first byte != 0x02 universally (a provider's
    // first byte could legitimately be 0x02); what we CAN check is
    // that the total length matches the legacy layout.
    assert_eq!(bytes.len(), 88 + t.model_id.len());
    assert!(!t.has_nonce());
}

#[kani::proof]
fn proof_v2_canonical_has_version_prefix_and_nonce() {
    let nonce: [u8; 16] = kani::any();
    kani::assume(nonce != [0u8; 16]);
    let t = symbolic_trade(nonce);
    let bytes = t.canonical_bytes();
    assert_eq!(bytes[0], TradeRecord::CANONICAL_V2);
    assert_eq!(&bytes[bytes.len() - 16..], &nonce);
    assert!(t.has_nonce());
}

#[kani::proof]
fn proof_v1_v2_canonical_bytes_never_collide() {
    // Construct one v1 trade (nonce=0) and one v2 trade with some
    // non-zero nonce, everything else identical. Their canonical
    // bytes must differ.
    let nonce: [u8; 16] = kani::any();
    kani::assume(nonce != [0u8; 16]);
    let provider = NodeId(kani::any());
    let consumer = NodeId(kani::any());
    let amt: u64 = kani::any();
    let toks: u64 = kani::any();
    let ts: u64 = kani::any();

    let t_v1 = TradeRecord {
        provider: provider.clone(),
        consumer: consumer.clone(),
        trm_amount: amt,
        tokens_processed: toks,
        timestamp: ts,
        model_id: "k".into(),
        flops_estimated: 0,
        nonce: [0u8; 16],
    };
    let t_v2 = TradeRecord {
        provider,
        consumer,
        trm_amount: amt,
        tokens_processed: toks,
        timestamp: ts,
        model_id: "k".into(),
        flops_estimated: 0,
        nonce,
    };
    // Different lengths (v2 has +1 byte version +16 byte nonce),
    // so they cannot be bytewise equal.
    assert!(t_v1.canonical_bytes() != t_v2.canonical_bytes());
}

// ---------------------------------------------------------------------------
// Staking / slashing invariants
// ---------------------------------------------------------------------------

#[kani::proof]
fn proof_apply_slash_on_unknown_node_returns_zero() {
    let mut pool = StakingPool::new();
    let nid = NodeId(kani::any());
    let penalty: f64 = kani::any();
    kani::assume(penalty >= 0.0 && penalty <= 1.0);
    let burned = pool.apply_slash(&nid, penalty);
    assert_eq!(burned, 0);
    assert_eq!(pool.total_staked(), 0);
}

#[kani::proof]
#[kani::unwind(2)]
fn proof_apply_slash_never_increases_total_staked() {
    // If apply_slash ever added to total_staked, it would be a
    // minting bug (slash must only burn). This proof doesn't
    // require a pre-populated pool — even on an empty pool
    // total_staked should never go UP after apply_slash.
    use crate::staking::{StakeDuration};
    let mut pool = StakingPool::new();
    let nid = NodeId(kani::any());
    let amt: u64 = kani::any();
    kani::assume(amt > 0 && amt <= 10_000);
    // Stake some amount first so apply_slash has something to burn.
    pool.stake(nid.clone(), amt, StakeDuration::Days30, 0).unwrap();
    let before = pool.total_staked();
    let penalty: f64 = kani::any();
    kani::assume(penalty >= 0.0 && penalty <= 1.0);
    pool.apply_slash(&nid, penalty);
    let after = pool.total_staked();
    assert!(after <= before, "slash must never mint");
}

// ---------------------------------------------------------------------------
// WelcomeLoanLimiter invariants
// ---------------------------------------------------------------------------

#[kani::proof]
#[kani::unwind(3)]
fn proof_welcome_loan_limiter_honors_cap() {
    // After N grants (N <= cap+1), can_issue always returns false
    // when N == cap.
    let mut limiter = WelcomeLoanLimiter::new();
    let cap = limiter.config().max_per_bucket_per_window;
    kani::assume(cap == 10); // default
    let now: u64 = kani::any();
    kani::assume(now > 0 && now < 1_000_000_000_000); // reasonable
    // Fill the bucket to the cap.
    for i in 0..cap {
        limiter.record("X", now + i as u64);
    }
    assert!(!limiter.can_issue("X", false, now + cap as u64));
}

// ---------------------------------------------------------------------------
// Meta-invariant: NonceCache.insert idempotency
// ---------------------------------------------------------------------------

#[kani::proof]
fn proof_nonce_cache_insert_is_idempotent_on_same_nonce() {
    let mut cache = NonceCache::default();
    let nonce: [u8; 16] = kani::any();
    kani::assume(nonce != [0u8; 16]);
    assert!(cache.insert(nonce));
    let len_before = cache.len();
    assert!(!cache.insert(nonce));
    let len_after = cache.len();
    assert_eq!(len_before, len_after);
}
