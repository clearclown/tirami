//! Adapter from forge-ledger state to forge-bank types.
//!
//! This is the single place where ComputeLedger lending state is reified
//! into forge-bank types. All `/v1/tirami/bank/*` handlers use it.

use tirami_bank::{
    FuturesContract, Portfolio, PortfolioManager, RiskTolerance, StrategyKind,
};
use tirami_core::NodeId;
use tirami_ledger::ComputeLedger;
use tirami_ledger::lending::{max_borrowable, offered_interest_rate};
use serde::{Deserialize, Serialize};

/// Wrapper combining PortfolioManager with a futures book.
/// PortfolioManager does not store futures, so we maintain them here.
pub struct BankServices {
    pub portfolio: PortfolioManager,
    pub futures: Vec<FuturesContract>,
    /// The strategy kind used when this service was last constructed/restored.
    /// Kept here so the snapshot can record it without needing dynamic dispatch downcast.
    pub strategy_kind: StrategyKind,
    /// Current risk tolerance, mirrored here for snapshot access.
    pub risk: RiskTolerance,
}

impl BankServices {
    /// Default initial state: 10,000 CU cash, balanced strategy, balanced risk.
    pub fn new_default() -> Self {
        let strategy_kind = StrategyKind::default();
        let risk = RiskTolerance::Balanced;
        let portfolio = Portfolio::new(10_000);
        let strategy = strategy_kind.to_strategy().expect("default StrategyKind is always valid");
        let mgr = PortfolioManager::new(portfolio, strategy, risk.clone());
        Self {
            portfolio: mgr,
            futures: Vec::new(),
            strategy_kind,
            risk,
        }
    }

    /// Produce a serializable snapshot of the current state.
    pub fn snapshot(&self) -> BankServicesSnapshot {
        BankServicesSnapshot {
            portfolio: self.portfolio.portfolio.clone(),
            futures: self.futures.clone(),
            risk: self.risk.clone(),
            strategy: self.strategy_kind.clone(),
            tick_history: self.portfolio.decision_history.clone(),
        }
    }

    /// Reconstruct a `BankServices` from a snapshot.
    ///
    /// Returns `Err` if the snapshot's strategy kind contains out-of-range parameters.
    pub fn from_snapshot(snap: BankServicesSnapshot) -> Result<Self, tirami_bank::BankError> {
        let strategy_kind = snap.strategy.clone();
        let risk = snap.risk.clone();
        let strategy = strategy_kind.to_strategy()?;
        let mut mgr = PortfolioManager::new(snap.portfolio, strategy, risk.clone());
        mgr.decision_history = snap.tick_history;
        Ok(Self {
            portfolio: mgr,
            futures: snap.futures,
            strategy_kind,
            risk,
        })
    }
}

/// Serializable snapshot of `BankServices` state.
///
/// Phase 10 TODO: add HMAC-SHA256 integrity check if tampering becomes a concern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankServicesSnapshot {
    pub portfolio: Portfolio,
    pub futures: Vec<FuturesContract>,
    pub risk: RiskTolerance,
    pub strategy: StrategyKind,
    pub tick_history: Vec<(tirami_bank::PoolSnapshot, Vec<tirami_bank::Decision>)>,
}

/// Build a fresh forge-bank PoolSnapshot from the current ledger state for a given local node.
///
/// Mirrors the existing `pool_handler` in api.rs: same data sources,
/// packaged into the forge-bank type.
pub fn pool_snapshot_from_ledger(
    ledger: &ComputeLedger,
    local_node_id: &NodeId,
) -> tirami_bank::PoolSnapshot {
    let status = ledger.lending_pool_status();
    let credit = ledger.compute_credit_score(local_node_id);
    let your_max_borrow = max_borrowable(credit, status.available_cu);
    let your_offered = offered_interest_rate(credit);

    tirami_bank::PoolSnapshot::new(
        status.total_pool_cu,
        status.lent_cu,
        status.available_cu,
        status.reserve_ratio,
        status.active_loan_count as u64,
        status.avg_interest_rate,
        your_max_borrow,
        your_offered,
    )
    .expect("ledger state must produce valid PoolSnapshot")
}
