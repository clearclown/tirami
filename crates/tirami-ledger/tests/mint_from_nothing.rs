//! Can a pair of cooperating identities create TRM without doing any work?
//!
//! Today TRM has no external value, so nobody gains from trying. The question
//! matters because the owner wants to make TRM tradeable on-chain, and that
//! inverts the threat model: the moment a unit is worth money, fabricating one
//! is worth doing.
//!
//! `execute_trade` rejects exactly two things — a zero amount and a self-trade
//! (`ledger.rs`). It does not check that the consumer can pay. `NodeBalance::
//! balance()` is `contributed as i64 - consumed as i64`, so it may go negative.

use ed25519_dalek::{Signer, SigningKey};
use tirami_core::NodeId;
use tirami_ledger::{ComputeLedger, SignedTradeRecord, TradeRecord};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn keypair(seed: u8) -> (SigningKey, NodeId) {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let node = NodeId(key.verifying_key().to_bytes());
    (key, node)
}

fn fabricate(
    provider_key: &SigningKey,
    provider: &NodeId,
    consumer_key: &SigningKey,
    consumer: &NodeId,
    amount: u64,
    nonce_seed: u8,
) -> SignedTradeRecord {
    let trade = TradeRecord {
        provider: provider.clone(),
        consumer: consumer.clone(),
        trm_amount: amount,
        tokens_processed: 0,
        model_id: "nothing-was-computed".to_string(),
        timestamp: now_ms(),
        flops_estimated: 0,
        nonce: [nonce_seed; 16],
    };
    let canonical = trade.canonical_bytes();
    SignedTradeRecord {
        trade,
        provider_sig: provider_key.sign(&canonical).to_bytes().to_vec(),
        consumer_sig: consumer_key.sign(&canonical).to_bytes().to_vec(),
        attestation: None,
    }
}

/// The attack, in full: one attacker, one throwaway identity, no inference.
///
/// Both signatures are genuine — the attacker holds both keys — so signature
/// verification is not a defence here. It never was: it proves two keys agreed,
/// not that a GPU ran.
#[test]
fn a_throwaway_identity_lets_an_attacker_mint_trm_without_computing() {
    let (attacker_key, attacker) = keypair(0xA1);
    let (burner_key, burner) = keypair(0xB1);

    let mut ledger = ComputeLedger::new();
    let before = ledger.get_balance(&attacker).map(|b| b.balance()).unwrap_or(0);

    let fake = fabricate(&attacker_key, &attacker, &burner_key, &burner, 1_000_000, 0x01);
    ledger
        .execute_signed_trade(&fake)
        .expect("a fully-signed trade for work that never happened is accepted");

    let after = ledger.get_balance(&attacker).expect("attacker now has a balance");
    assert_eq!(
        after.balance() - before,
        1_000_000,
        "attacker gained TRM without performing any inference"
    );
    assert_eq!(
        after.contributed, 1_000_000,
        "the ledger credits this as contributed work"
    );

    let burner_balance = ledger.get_balance(&burner).expect("burner exists").balance();
    assert!(
        burner_balance < 0,
        "the cost lands on an identity that is simply abandoned (balance {burner_balance})"
    );
}

/// Repeating it with fresh burners is unbounded. A keypair costs nothing, and
/// each one absorbs the debit once before being discarded — so the pattern is
/// not the recurring-counterparty shape the collusion detector looks for.
#[test]
fn repeating_with_fresh_burners_scales_without_limit() {
    let (attacker_key, attacker) = keypair(0xA2);
    let mut ledger = ComputeLedger::new();

    for i in 0..20u8 {
        let (burner_key, burner) = keypair(0x40 + i);
        let fake = fabricate(&attacker_key, &attacker, &burner_key, &burner, 500_000, i);
        ledger
            .execute_signed_trade(&fake)
            .expect("each fabricated trade is accepted");
    }

    let total = ledger.get_balance(&attacker).expect("attacker").balance();
    assert_eq!(
        total, 10_000_000,
        "20 fabrications produced {total} TRM from nothing"
    );
}

/// The one guard that does exist. It is defeated by a second keypair, which is
/// free to generate — so it raises the cost of the attack by nothing.
#[test]
fn the_self_trade_guard_is_the_only_thing_stopping_this() {
    let (key, node) = keypair(0xC3);
    let mut ledger = ComputeLedger::new();

    let self_trade = fabricate(&key, &node, &key, &node, 1_000_000, 0x02);
    let _ = ledger.execute_signed_trade(&self_trade);

    assert_eq!(
        ledger.get_balance(&node).map(|b| b.balance()).unwrap_or(0),
        0,
        "trading with yourself is refused"
    );
}
