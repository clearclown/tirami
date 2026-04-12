//! Payment types and CU ↔ satoshi conversion.
//!
//! The protocol settles in CU. This module converts CU to sats
//! for Lightning settlement, keeping the boundary clean.

use tirami_core::NodeId;

/// A request to create an invoice for inference payment.
#[derive(Debug, Clone)]
pub struct InvoiceRequest {
    /// Who is paying (consumer node).
    pub consumer: NodeId,
    /// Who is being paid (provider node).
    pub provider: NodeId,
    /// Amount in CU.
    pub trm_amount: u64,
    /// Description of the inference work.
    pub description: String,
}

/// Result of a payment attempt.
#[derive(Debug, Clone)]
pub enum PaymentResult {
    /// Payment succeeded.
    Success {
        payment_id: String,
        amount_msats: u64,
    },
    /// Payment failed.
    Failed { reason: String },
    /// Payment is pending (in-flight).
    Pending { payment_id: String },
}

/// Exchange rate: CU to millisatoshis.
///
/// This is configurable per-node and per-settlement-window.
/// Default: 1 CU = 10 msats (0.01 sats) — roughly $0.0000001 at $100k/BTC.
/// This means 1 million CU ≈ 10,000 sats ≈ $0.10.
#[derive(Debug, Clone)]
pub struct ExchangeRate {
    /// Millisatoshis per CU.
    pub msats_per_cu: u64,
}

impl Default for ExchangeRate {
    fn default() -> Self {
        Self { msats_per_cu: 10 }
    }
}

impl ExchangeRate {
    /// Convert CU amount to millisatoshis.
    pub fn cu_to_msats(&self, cu: u64) -> u64 {
        cu.saturating_mul(self.msats_per_cu)
    }

    /// Convert millisatoshis to CU amount.
    pub fn msats_to_cu(&self, msats: u64) -> u64 {
        if self.msats_per_cu == 0 {
            return 0;
        }
        msats / self.msats_per_cu
    }
}

/// Pricing summary for display to users.
#[derive(Debug, Clone)]
pub struct PricingSummary {
    /// CU per token (from market price).
    pub trm_per_token: f64,
    /// Satoshis per token (derived).
    pub sats_per_token: f64,
    /// USD per token (estimated, if BTC price available).
    pub usd_per_token: Option<f64>,
}

impl PricingSummary {
    pub fn from_rate(trm_per_token: f64, rate: &ExchangeRate, btc_usd: Option<f64>) -> Self {
        let msats_per_token = trm_per_token * rate.msats_per_cu as f64;
        let sats_per_token = msats_per_token / 1000.0;
        let usd_per_token = btc_usd.map(|price| sats_per_token / 100_000_000.0 * price);

        Self {
            trm_per_token,
            sats_per_token,
            usd_per_token,
        }
    }
}

/// Result of computing a settlement invoice.
#[derive(Debug, Clone)]
pub struct SettlementInvoice {
    /// Net CU owed to the provider.
    pub net_cu: u64,
    /// Equivalent amount in millisatoshis.
    pub amount_msats: u64,
    /// Equivalent amount in satoshis.
    pub amount_sats: u64,
    /// Description for the Lightning invoice.
    pub description: String,
}

/// Tracks payment status for settlements (Issue #15).
#[derive(Debug, Clone)]
pub enum SettlementPaymentStatus {
    /// Invoice created, awaiting payment.
    Pending { bolt11: String, expires_at: u64 },
    /// Payment confirmed with proof.
    Paid { payment_hash: String, paid_at: u64 },
    /// Payment expired without settlement.
    Expired,
    /// Payment explicitly cancelled.
    Cancelled,
}

/// A tracked settlement with payment lifecycle (Issue #15).
#[derive(Debug, Clone)]
pub struct TrackedSettlement {
    pub invoice: SettlementInvoice,
    pub status: SettlementPaymentStatus,
    pub created_at: u64,
}

/// Create a settlement invoice from a settlement statement's net CU.
///
/// Only nodes with positive net CU (providers) get invoices.
/// Returns None if net_cu is zero or negative.
pub fn create_settlement_invoice(
    net_cu: i64,
    rate: &ExchangeRate,
    window_hours: u64,
) -> Option<SettlementInvoice> {
    if net_cu <= 0 {
        return None;
    }

    let cu = net_cu as u64;
    let amount_msats = rate.cu_to_msats(cu);
    let amount_sats = amount_msats / 1000;

    Some(SettlementInvoice {
        net_cu: cu,
        amount_msats,
        amount_sats,
        description: format!(
            "Forge settlement: {} CU over {}h",
            cu, window_hours
        ),
    })
}

// ---------------------------------------------------------------------------
// BTC → CU deposit flow (Phase 5.5/6)
// ---------------------------------------------------------------------------

/// Default exchange rate: 10 millisats per CU.
/// Matches forge-economics/spec/parameters.md cloud API anchor and the
/// default on [`ExchangeRate`].
pub const DEFAULT_MSATS_PER_CU: u64 = 10;

/// Errors that can arise while creating or settling a CU deposit.
#[derive(Debug, Clone)]
pub enum LightningError {
    /// The requested CU amount was zero.
    ZeroAmount,
    /// The exchange rate was zero (would make the invoice free).
    ZeroRate,
    /// Amount calculation overflowed u64.
    Overflow,
    /// Underlying Lightning backend failed.
    Backend(String),
}

impl std::fmt::Display for LightningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LightningError::ZeroAmount => write!(f, "trm_amount must be greater than zero"),
            LightningError::ZeroRate => write!(f, "msats_per_cu must be greater than zero"),
            LightningError::Overflow => write!(f, "invoice amount overflowed u64"),
            LightningError::Backend(msg) => write!(f, "lightning backend error: {}", msg),
        }
    }
}

impl std::error::Error for LightningError {}

/// Record of a CU deposit created by paying a Lightning invoice.
///
/// When a human or agent wants to credit CU to a Forge node, they request
/// a Lightning invoice denominated in msats; upon payment, the corresponding
/// CU amount is credited to the recipient's balance.
#[derive(Debug, Clone)]
pub struct CuDeposit {
    /// Recipient node that will be credited.
    pub recipient: tirami_core::NodeId,
    /// Amount of CU to credit upon successful payment.
    pub trm_amount: u64,
    /// Millisats paid for this deposit.
    pub msats: u64,
    /// Exchange rate used: msats per CU.
    pub msats_per_cu: u64,
    /// BOLT11 invoice for the human/agent to pay.
    pub invoice: String,
    /// When the deposit request was created (milliseconds since epoch).
    pub created_at: u64,
    /// Whether payment has been confirmed and CU credited.
    pub settled: bool,
}

/// Bidirectional exchange rate summary for display.
#[derive(Debug, Clone)]
pub struct ExchangeRateSummary {
    pub msats_per_cu: u64,
    pub cu_per_btc: u64,
}

/// Convert CU to millisats using the default rate.
pub fn cu_to_msats(cu: u64) -> u64 {
    cu.saturating_mul(DEFAULT_MSATS_PER_CU)
}

/// Convert millisats to CU using the default rate.
pub fn msats_to_cu(msats: u64) -> u64 {
    msats / DEFAULT_MSATS_PER_CU
}

/// Compute a bidirectional exchange rate summary for display.
pub fn exchange_rate_summary() -> ExchangeRateSummary {
    ExchangeRateSummary {
        msats_per_cu: DEFAULT_MSATS_PER_CU,
        // 1 BTC = 100_000_000 sats = 100_000_000_000 msats.
        cu_per_btc: 100_000_000_000 / DEFAULT_MSATS_PER_CU,
    }
}

/// Current unix time in milliseconds, or 0 if the clock is before the epoch.
fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Create a CU deposit request. Generates a Lightning invoice that, when
/// paid, will credit `trm_amount` to the `recipient` node's balance.
///
/// This is the "BTC → CU" direction of the bridge. The counterpart is the
/// existing settlement-invoice flow (for CU → BTC cash-out).
///
/// The caller is responsible for transmitting the invoice to the payer and
/// for invoking [`credit_from_invoice`] once payment is confirmed (via the
/// Lightning node's `payment_received` callback). The returned BOLT11 string
/// is a placeholder in this stubbed bridge; the production implementation
/// will delegate to an ldk-node instance.
pub fn create_deposit(
    recipient: tirami_core::NodeId,
    trm_amount: u64,
    msats_per_cu: Option<u64>,
) -> Result<CuDeposit, LightningError> {
    if trm_amount == 0 {
        return Err(LightningError::ZeroAmount);
    }
    let rate = msats_per_cu.unwrap_or(DEFAULT_MSATS_PER_CU);
    if rate == 0 {
        return Err(LightningError::ZeroRate);
    }
    let msats = trm_amount
        .checked_mul(rate)
        .ok_or(LightningError::Overflow)?;

    // Placeholder BOLT11 invoice. The real bridge will call into ldk-node.
    let invoice = format!("lnbc_deposit_placeholder_{}cu", trm_amount);

    Ok(CuDeposit {
        recipient,
        trm_amount,
        msats,
        msats_per_cu: rate,
        invoice,
        created_at: now_millis(),
        settled: false,
    })
}

/// Mark a deposit as settled. Should be called when the Lightning node
/// confirms that the invoice has been paid.
///
/// Returns the CU amount that should be credited to the recipient.
/// Caller (forge-node daemon) is responsible for calling
/// `ComputeLedger::credit_from_bridge(recipient, trm_amount)`.
pub fn credit_from_invoice(deposit: &mut CuDeposit) -> u64 {
    deposit.settled = true;
    deposit.trm_amount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_rate_conversion() {
        let rate = ExchangeRate::default(); // 10 msats/CU
        assert_eq!(rate.cu_to_msats(100), 1000);
        assert_eq!(rate.msats_to_cu(1000), 100);
    }

    #[test]
    fn pricing_summary_calculation() {
        let rate = ExchangeRate { msats_per_cu: 10 };
        let summary = PricingSummary::from_rate(1.0, &rate, Some(100_000.0));
        assert!((summary.sats_per_token - 0.01).abs() < 1e-6);
        assert!(summary.usd_per_token.unwrap() < 0.001);
    }

    #[test]
    fn cu_to_msats_round_trips_at_default_rate() {
        assert_eq!(cu_to_msats(1_000), 10_000);
        assert_eq!(msats_to_cu(10_000), 1_000);
    }

    #[test]
    fn create_deposit_builds_invoice() {
        let recipient = tirami_core::NodeId([1u8; 32]);
        let deposit = create_deposit(recipient.clone(), 5_000, None).expect("deposit");
        assert_eq!(deposit.recipient, recipient);
        assert_eq!(deposit.trm_amount, 5_000);
        assert_eq!(deposit.msats, 50_000);
        assert_eq!(deposit.msats_per_cu, DEFAULT_MSATS_PER_CU);
        assert!(!deposit.settled);
        assert!(!deposit.invoice.is_empty());
    }

    #[test]
    fn credit_from_invoice_marks_settled_and_returns_amount() {
        let recipient = tirami_core::NodeId([2u8; 32]);
        let mut deposit = create_deposit(recipient, 2_500, None).expect("deposit");
        let cu = credit_from_invoice(&mut deposit);
        assert_eq!(cu, 2_500);
        assert!(deposit.settled);
    }

    #[test]
    fn exchange_rate_summary_is_consistent() {
        let summary = exchange_rate_summary();
        assert_eq!(summary.msats_per_cu, DEFAULT_MSATS_PER_CU);
        assert_eq!(summary.cu_per_btc, 10_000_000_000);
    }

    #[test]
    fn create_deposit_with_custom_rate() {
        let recipient = tirami_core::NodeId([3u8; 32]);
        let deposit = create_deposit(recipient, 1_000, Some(20)).expect("deposit");
        assert_eq!(deposit.msats, 20_000);
        assert_eq!(deposit.msats_per_cu, 20);
    }
}
