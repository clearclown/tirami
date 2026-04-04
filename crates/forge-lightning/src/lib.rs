//! Bitcoin Lightning Network integration for Forge.
//!
//! Enables real Bitcoin micropayments for inference:
//! - Providers earn sats by serving inference
//! - Consumers pay sats for inference
//! - No custodian, no KYC — self-sovereign Lightning node
//!
//! Uses LDK (Lightning Development Kit) via ldk-node for a lightweight
//! embedded Lightning node that doesn't require Bitcoin Core.

pub mod node;
pub mod payment;

pub use node::ForgeWallet;
pub use payment::{
    ExchangeRate, InvoiceRequest, PaymentResult, PricingSummary,
    SettlementInvoice, SettlementPaymentStatus, TrackedSettlement,
};
