//! Payment types and CU ↔ satoshi conversion.
//!
//! The protocol settles in CU. This module converts CU to sats
//! for Lightning settlement, keeping the boundary clean.

use forge_core::NodeId;

/// A request to create an invoice for inference payment.
#[derive(Debug, Clone)]
pub struct InvoiceRequest {
    /// Who is paying (consumer node).
    pub consumer: NodeId,
    /// Who is being paid (provider node).
    pub provider: NodeId,
    /// Amount in CU.
    pub cu_amount: u64,
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
    pub cu_per_token: f64,
    /// Satoshis per token (derived).
    pub sats_per_token: f64,
    /// USD per token (estimated, if BTC price available).
    pub usd_per_token: Option<f64>,
}

impl PricingSummary {
    pub fn from_rate(cu_per_token: f64, rate: &ExchangeRate, btc_usd: Option<f64>) -> Self {
        let msats_per_token = cu_per_token * rate.msats_per_cu as f64;
        let sats_per_token = msats_per_token / 1000.0;
        let usd_per_token = btc_usd.map(|price| sats_per_token / 100_000_000.0 * price);

        Self {
            cu_per_token,
            sats_per_token,
            usd_per_token,
        }
    }
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
}
