//! Integration test: full Phase 5.5 loan flow from creation to gossip propagation.
//!
//! This test does NOT use real network transport — it simulates the wire layer
//! by directly reconstructing a `SignedLoanRecord` on the receiving side from
//! the same fields that would travel in a `LoanGossip` wire message, and passing
//! it to a second ledger's `create_loan`. The goal is to verify that the same
//! signed loan produces identical state on both nodes.
//!
//! Note: `forge-proto::messages::LoanGossip` is intentionally NOT imported here
//! because `forge-proto` is not a dependency of `forge-ledger` (and adding it
//! would create a dependency cycle). Instead we mimic its field-for-field
//! representation locally — which is exactly what `handle_loan_gossip` does on
//! the receiving end in `forge-net`.

use ed25519_dalek::{Signer, SigningKey};
use tirami_core::{LayerRange, ModelId, NodeId, WorkUnit};
use tirami_ledger::{
    ComputeLedger, LoanRecord, LoanStatus, SignedLoanRecord,
    ledger::FLOPS_PER_CU,
    lending::{COLD_START_CREDIT, WELCOME_LOAN_AMOUNT, offered_interest_rate},
};
use rand::rngs::OsRng;

/// Local stand-in for `tirami_proto::messages::LoanGossip`. Field-identical to
/// the real wire type so this test documents the exact wire shape expected by
/// `handle_loan_gossip`.
#[derive(Debug, Clone)]
struct LoanGossipWire {
    lender: NodeId,
    borrower: NodeId,
    principal_trm: u64,
    interest_rate_per_hour: f64,
    term_hours: u64,
    collateral_trm: u64,
    created_at: u64,
    due_at: u64,
    lender_sig: Vec<u8>,
    borrower_sig: Vec<u8>,
}

/// Generate a fresh Ed25519 keypair and a NodeId derived from it.
fn fresh_identity() -> (SigningKey, NodeId) {
    let key = SigningKey::generate(&mut OsRng);
    let node_id = NodeId(key.verifying_key().to_bytes());
    (key, node_id)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Seed a ledger with `cu` worth of contributed compute for `node_id`, so the
/// borrower has enough available balance for collateral reservation.
fn seed_contribution(ledger: &mut ComputeLedger, node_id: &NodeId, cu: u64) {
    ledger.record_contribution(WorkUnit {
        node_id: node_id.clone(),
        timestamp: now_ms(),
        layers_computed: LayerRange::new(0, 1),
        model_id: ModelId("test-model".to_string()),
        tokens_processed: 0,
        estimated_flops: cu * FLOPS_PER_CU,
    });
}

#[test]
fn loan_propagates_from_one_ledger_to_another() {
    // Two independent nodes.
    let mut ledger_a = ComputeLedger::new();
    let mut ledger_b = ComputeLedger::new();

    // Fresh keys for the identity. We use a single identity serving as both
    // lender and borrower so the self-signed MVP loan verifies without
    // bootstrapping external trade history.
    let (signing_key, node_id) = fresh_identity();

    // Seed both ledgers with enough CU for the borrower to cover collateral.
    // collateral = WELCOME_LOAN_AMOUNT * 3 = 3_000; give 5_000 of headroom.
    seed_contribution(&mut ledger_a, &node_id, 5_000);
    seed_contribution(&mut ledger_b, &node_id, 5_000);

    // Build a loan using the welcome-loan principal at the cold-start rate.
    let created_at = now_ms();
    let term_hours = 24u64;
    let due_at = created_at + term_hours * 3_600_000;

    let mut loan = LoanRecord {
        loan_id: [0u8; 32],
        lender: node_id.clone(),
        borrower: node_id.clone(),
        principal_trm: WELCOME_LOAN_AMOUNT,
        interest_rate_per_hour: offered_interest_rate(COLD_START_CREDIT),
        term_hours,
        // 3:1 collateral = MAX_LTV_RATIO boundary.
        collateral_trm: WELCOME_LOAN_AMOUNT * 3,
        status: LoanStatus::Active,
        created_at,
        due_at,
        repaid_at: None,
    };
    loan.loan_id = loan.compute_loan_id();

    let canonical = loan.canonical_bytes();
    let sig = signing_key.sign(&canonical).to_bytes().to_vec();
    let signed = SignedLoanRecord {
        loan: loan.clone(),
        lender_sig: sig.clone(),
        borrower_sig: sig.clone(),
    };

    // Dual-signature verification round-trip.
    signed.verify().expect("self-signed loan must verify");

    // Apply to ledger A.
    ledger_a
        .create_loan(signed.clone())
        .expect("ledger A should accept the first loan (cold-start credit > min)");

    // Ledger A now has the loan.
    let loans_a = ledger_a.active_loans_for(&node_id);
    assert_eq!(loans_a.len(), 1, "ledger A should have 1 active loan");
    assert_eq!(loans_a[0].loan.loan_id, signed.loan.loan_id);

    // ---------------------------------------------------------------
    // Simulate wire transmission: build the gossip wire message from the
    // SignedLoanRecord (mirrors what `broadcast_loan` does in forge-net).
    // ---------------------------------------------------------------
    let wire = LoanGossipWire {
        lender: signed.loan.lender.clone(),
        borrower: signed.loan.borrower.clone(),
        principal_trm: signed.loan.principal_trm,
        interest_rate_per_hour: signed.loan.interest_rate_per_hour,
        term_hours: signed.loan.term_hours,
        collateral_trm: signed.loan.collateral_trm,
        created_at: signed.loan.created_at,
        due_at: signed.loan.due_at,
        lender_sig: signed.lender_sig.clone(),
        borrower_sig: signed.borrower_sig.clone(),
    };

    // Reconstruct the SignedLoanRecord on the receiving side (Node B)
    // following the same logic as `handle_loan_gossip` in forge-net.
    let mut reconstructed_loan = LoanRecord {
        loan_id: [0u8; 32],
        lender: wire.lender.clone(),
        borrower: wire.borrower.clone(),
        principal_trm: wire.principal_trm,
        interest_rate_per_hour: wire.interest_rate_per_hour,
        term_hours: wire.term_hours,
        collateral_trm: wire.collateral_trm,
        status: LoanStatus::Active,
        created_at: wire.created_at,
        due_at: wire.due_at,
        repaid_at: None,
    };
    reconstructed_loan.loan_id = reconstructed_loan.compute_loan_id();

    let reconstructed_signed = SignedLoanRecord {
        loan: reconstructed_loan,
        lender_sig: wire.lender_sig.clone(),
        borrower_sig: wire.borrower_sig.clone(),
    };
    reconstructed_signed
        .verify()
        .expect("reconstructed signed loan must verify");

    // Apply to ledger B.
    ledger_b
        .create_loan(reconstructed_signed.clone())
        .expect("ledger B should accept the gossiped loan");

    // Both ledgers now know about the same loan.
    let loans_b = ledger_b.active_loans_for(&node_id);
    assert_eq!(loans_b.len(), 1, "ledger B should have 1 active loan");
    assert_eq!(loans_b[0].loan.loan_id, signed.loan.loan_id);
    assert_eq!(loans_b[0].loan.principal_trm, WELCOME_LOAN_AMOUNT);
    assert_eq!(loans_b[0].loan.status, LoanStatus::Active);

    // Deterministic loan_id: must match across A and B because canonical_bytes
    // only hashes the signed, wire-transmitted fields.
    assert_eq!(
        loans_a[0].loan.loan_id, loans_b[0].loan.loan_id,
        "loan_id should be deterministic across nodes"
    );
    // And must match the signatures bit-for-bit.
    assert_eq!(loans_a[0].lender_sig, loans_b[0].lender_sig);
    assert_eq!(loans_a[0].borrower_sig, loans_b[0].borrower_sig);
}

#[test]
fn loan_gossip_rejects_tampered_signature() {
    let mut ledger = ComputeLedger::new();
    let (signing_key, node_id) = fresh_identity();

    let created_at = now_ms();
    let mut loan = LoanRecord {
        loan_id: [0u8; 32],
        lender: node_id.clone(),
        borrower: node_id.clone(),
        principal_trm: 500,
        interest_rate_per_hour: 0.001,
        term_hours: 24,
        collateral_trm: 1_500,
        status: LoanStatus::Active,
        created_at,
        due_at: created_at + 24 * 3_600_000,
        repaid_at: None,
    };
    loan.loan_id = loan.compute_loan_id();
    let canonical = loan.canonical_bytes();
    let mut sig = signing_key.sign(&canonical).to_bytes().to_vec();

    // Tamper with the signature.
    sig[0] ^= 0xFF;

    let signed = SignedLoanRecord {
        loan: loan.clone(),
        lender_sig: sig.clone(),
        borrower_sig: sig,
    };

    // Direct verify() must fail.
    assert!(
        signed.verify().is_err(),
        "tampered signature must fail verify"
    );

    // And create_loan must propagate the signature error.
    assert!(
        ledger.create_loan(signed).is_err(),
        "ledger must reject tampered loan"
    );
}

#[test]
fn loan_gossip_preserves_repay_flow() {
    // After a loan is repaid on Node A, its lifecycle should transition
    // cleanly and the loan should no longer appear in `active_loans_for`.
    let mut ledger_a = ComputeLedger::new();
    let (key, node_id) = fresh_identity();
    // Collateral = 1_500, plus repay must cover principal + interest; seed
    // generously so repay_loan's balance check (can_afford(total_due)) passes.
    seed_contribution(&mut ledger_a, &node_id, 5_000);

    let created_at = now_ms();
    let mut loan = LoanRecord {
        loan_id: [0u8; 32],
        lender: node_id.clone(),
        borrower: node_id.clone(),
        principal_trm: 500,
        interest_rate_per_hour: 0.001,
        term_hours: 1,
        collateral_trm: 1_500,
        status: LoanStatus::Active,
        created_at,
        due_at: created_at + 3_600_000,
        repaid_at: None,
    };
    loan.loan_id = loan.compute_loan_id();
    let canonical = loan.canonical_bytes();
    let sig = key.sign(&canonical).to_bytes().to_vec();
    let loan_id = loan.loan_id;
    let signed = SignedLoanRecord {
        loan,
        lender_sig: sig.clone(),
        borrower_sig: sig,
    };

    ledger_a
        .create_loan(signed)
        .expect("loan should be created");
    assert_eq!(ledger_a.active_loans_for(&node_id).len(), 1);

    // Repay.
    ledger_a
        .repay_loan(&loan_id)
        .expect("repay should succeed");

    // After repay, the loan transitions to Repaid and no longer counts as
    // an active loan for either party.
    let active = ledger_a.active_loans_for(&node_id);
    assert_eq!(
        active.len(),
        0,
        "repaid loan must not appear in active_loans_for"
    );
}
