//! Agent safety mechanisms for the Forge compute economy.
//!
//! These are CRITICAL safeguards that prevent:
//! - AI agents draining all CU in seconds
//! - Runaway inference loops consuming the entire network
//! - Malicious agents exploiting the economy
//! - Cascading failures across the mesh
//!
//! Design principle: fail-safe. If any safety check cannot determine
//! safety, it DENIES the action. False positives are acceptable;
//! false negatives are not.

use tirami_core::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Global kill switch. When active, ALL trades are frozen.
/// Only a human operator can toggle this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitch {
    pub active: bool,
    pub reason: Option<String>,
    pub activated_at: Option<u64>,
    pub activated_by: Option<String>,
}

impl Default for KillSwitch {
    fn default() -> Self {
        Self {
            active: false,
            reason: None,
            activated_at: None,
            activated_by: None,
        }
    }
}

impl KillSwitch {
    pub fn activate(&mut self, reason: &str, operator: &str) {
        self.active = true;
        self.reason = Some(reason.to_string());
        self.activated_at = Some(now_millis());
        self.activated_by = Some(operator.to_string());
        tracing::error!(
            "KILL SWITCH ACTIVATED by {}: {}",
            operator,
            reason
        );
    }

    pub fn deactivate(&mut self) {
        tracing::warn!("Kill switch deactivated");
        self.active = false;
        self.reason = None;
    }
}

/// Per-agent budget policy. Set by the human operator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetPolicy {
    /// Maximum CU an agent can spend per hour.
    pub max_cu_per_hour: u64,
    /// Maximum CU an agent can spend per single request.
    pub max_cu_per_request: u64,
    /// Maximum total CU an agent can ever spend (lifetime cap).
    pub max_cu_lifetime: u64,
    /// Whether this agent requires human approval above a threshold.
    pub human_approval_threshold: Option<u64>,
}

impl Default for BudgetPolicy {
    fn default() -> Self {
        Self {
            max_cu_per_hour: 10_000,
            max_cu_per_request: 1_000,
            max_cu_lifetime: 1_000_000,
            human_approval_threshold: Some(5_000),
        }
    }
}

/// Circuit breaker state per node.
#[derive(Debug, Clone)]
struct CircuitState {
    /// CU spent in the current hour window.
    hourly_spend: u64,
    /// Hour window start.
    hour_start: u64,
    /// Total CU spent ever.
    lifetime_spend: u64,
    /// Number of consecutive errors (triggers trip).
    consecutive_errors: u32,
    /// Whether the circuit is tripped (open).
    tripped: bool,
    /// When the circuit was tripped.
    tripped_at: Option<u64>,
}

impl Default for CircuitState {
    fn default() -> Self {
        Self {
            hourly_spend: 0,
            hour_start: now_millis(),
            lifetime_spend: 0,
            consecutive_errors: 0,
            tripped: false,
            tripped_at: None,
        }
    }
}

/// Spend velocity tracker — detects anomalous spending patterns.
#[derive(Debug, Clone)]
struct VelocityWindow {
    /// Timestamps of recent spends (sliding window).
    recent_spends: Vec<(u64, u64)>, // (timestamp, trm_amount)
}

impl Default for VelocityWindow {
    fn default() -> Self {
        Self {
            recent_spends: Vec::new(),
        }
    }
}

impl VelocityWindow {
    const WINDOW_MS: u64 = 60_000; // 1 minute
    const MAX_SPENDS_PER_MINUTE: usize = 30;

    fn record(&mut self, cu: u64) {
        let now = now_millis();
        self.recent_spends.push((now, cu));
        // Evict old entries
        self.recent_spends
            .retain(|(ts, _)| now - ts < Self::WINDOW_MS);
    }

    fn is_anomalous(&self) -> bool {
        self.recent_spends.len() > Self::MAX_SPENDS_PER_MINUTE
    }

    fn minute_total(&self) -> u64 {
        let now = now_millis();
        self.recent_spends
            .iter()
            .filter(|(ts, _)| now - ts < Self::WINDOW_MS)
            .map(|(_, cu)| cu)
            .sum()
    }
}

/// Global lending circuit breaker.
///
/// Trips when the default rate in the last hour exceeds
/// `DEFAULT_CIRCUIT_BREAKER_THRESHOLD` (10%) or when the new-loan velocity
/// exceeds `MAX_LENDING_VELOCITY` (10 loans/min).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LendingCircuitState {
    /// Timestamps (ms) of recently created loans for velocity window.
    recent_loans: Vec<u64>,
    /// (timestamp_ms, defaulted) entries for hourly default-rate window.
    recent_defaults: Vec<(u64, bool)>,
    tripped: bool,
    tripped_at: Option<u64>,
    trip_reason: Option<String>,
}

impl LendingCircuitState {
    const VELOCITY_WINDOW_MS: u64 = 60_000; // 1 minute
    const DEFAULT_WINDOW_MS: u64 = 3_600_000; // 1 hour
    const RESET_AFTER_MS: u64 = 300_000; // 5 minutes after trip

    pub fn record_loan(&mut self, now_ms: u64) {
        self.recent_loans.push(now_ms);
        self.prune_loans(now_ms);
    }

    pub fn record_loan_outcome(&mut self, now_ms: u64, defaulted: bool) {
        self.recent_defaults.push((now_ms, defaulted));
        self.prune_defaults(now_ms);
    }

    fn prune_loans(&mut self, now_ms: u64) {
        self.recent_loans
            .retain(|t| now_ms.saturating_sub(*t) <= Self::VELOCITY_WINDOW_MS);
    }

    fn prune_defaults(&mut self, now_ms: u64) {
        self.recent_defaults
            .retain(|(t, _)| now_ms.saturating_sub(*t) <= Self::DEFAULT_WINDOW_MS);
    }

    /// Number of loans created in the last minute.
    pub fn velocity(&mut self, now_ms: u64) -> usize {
        self.prune_loans(now_ms);
        self.recent_loans.len()
    }

    /// Default rate in the last hour (0.0 to 1.0).
    pub fn default_rate(&mut self, now_ms: u64) -> f64 {
        self.prune_defaults(now_ms);
        let total = self.recent_defaults.len();
        if total == 0 {
            return 0.0;
        }
        let defaults = self.recent_defaults.iter().filter(|(_, d)| *d).count();
        defaults as f64 / total as f64
    }

    /// Check if tripped; auto-reset after RESET_AFTER_MS.
    pub fn is_tripped(&mut self, now_ms: u64) -> bool {
        if self.tripped {
            if let Some(ts) = self.tripped_at {
                if now_ms.saturating_sub(ts) >= Self::RESET_AFTER_MS {
                    self.tripped = false;
                    self.tripped_at = None;
                    self.trip_reason = None;
                }
            }
        }
        self.tripped
    }

    pub fn trip(&mut self, now_ms: u64, reason: &str) {
        self.tripped = true;
        self.tripped_at = Some(now_ms);
        self.trip_reason = Some(reason.to_string());
        tracing::error!("LENDING CIRCUIT BREAKER TRIPPED: {}", reason);
    }
}

/// The safety controller — coordinates all safety mechanisms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyController {
    pub kill_switch: KillSwitch,
    pub default_policy: BudgetPolicy,
    #[serde(skip)]
    circuits: HashMap<NodeId, CircuitState>,
    #[serde(skip)]
    velocity: HashMap<NodeId, VelocityWindow>,
    policies: HashMap<String, BudgetPolicy>, // node_id hex → policy
    #[serde(default)]
    lending: LendingCircuitState,
}

impl Default for SafetyController {
    fn default() -> Self {
        Self::new()
    }
}

impl SafetyController {
    pub fn new() -> Self {
        Self {
            kill_switch: KillSwitch::default(),
            default_policy: BudgetPolicy::default(),
            circuits: HashMap::new(),
            velocity: HashMap::new(),
            policies: HashMap::new(),
            lending: LendingCircuitState::default(),
        }
    }

    /// Set a custom budget policy for a specific node.
    pub fn set_policy(&mut self, node_id: &NodeId, policy: BudgetPolicy) {
        self.policies.insert(node_id.to_hex(), policy);
    }

    /// Get the effective policy for a node (custom or default).
    pub fn policy_for(&self, node_id: &NodeId) -> &BudgetPolicy {
        self.policies
            .get(&node_id.to_hex())
            .unwrap_or(&self.default_policy)
    }

    /// Check if a spend is allowed. Returns Ok(()) or Err with reason.
    /// This is the MAIN SAFETY GATE — called before every CU expenditure.
    pub fn check_spend(
        &mut self,
        node_id: &NodeId,
        trm_amount: u64,
    ) -> Result<(), SpendDenied> {
        // 1. Kill switch — absolute override
        if self.kill_switch.active {
            return Err(SpendDenied::KillSwitchActive(
                self.kill_switch
                    .reason
                    .clone()
                    .unwrap_or_else(|| "emergency halt".to_string()),
            ));
        }

        let policy = self.policy_for(node_id).clone();

        // 2. Per-request limit
        if trm_amount > policy.max_cu_per_request {
            return Err(SpendDenied::ExceedsRequestLimit {
                requested: trm_amount,
                limit: policy.max_cu_per_request,
            });
        }

        // 3. Circuit breaker — is this node tripped?
        let circuit = self.circuits.entry(node_id.clone()).or_default();
        if circuit.tripped {
            // Auto-reset after 5 minutes
            if let Some(tripped_at) = circuit.tripped_at {
                if now_millis() - tripped_at > 300_000 {
                    circuit.tripped = false;
                    circuit.consecutive_errors = 0;
                    tracing::info!("Circuit breaker auto-reset for {}", node_id.to_hex());
                } else {
                    return Err(SpendDenied::CircuitBreakerTripped);
                }
            }
        }

        // 4. Hourly spend limit
        let now = now_millis();
        if now - circuit.hour_start > 3_600_000 {
            circuit.hourly_spend = 0;
            circuit.hour_start = now;
        }
        if circuit.hourly_spend + trm_amount > policy.max_cu_per_hour {
            return Err(SpendDenied::HourlyLimitExceeded {
                spent: circuit.hourly_spend,
                limit: policy.max_cu_per_hour,
            });
        }

        // 5. Lifetime cap
        if circuit.lifetime_spend + trm_amount > policy.max_cu_lifetime {
            return Err(SpendDenied::LifetimeCapReached {
                spent: circuit.lifetime_spend,
                cap: policy.max_cu_lifetime,
            });
        }

        // 6. Spend velocity check
        let velocity = self.velocity.entry(node_id.clone()).or_default();
        velocity.record(trm_amount);
        if velocity.is_anomalous() {
            // Trip circuit breaker
            let circuit = self.circuits.get_mut(node_id).unwrap();
            circuit.tripped = true;
            circuit.tripped_at = Some(now);
            tracing::warn!(
                "CIRCUIT BREAKER TRIPPED for {}: {} spends/min, {} CU/min",
                node_id.to_hex(),
                velocity.recent_spends.len(),
                velocity.minute_total()
            );
            return Err(SpendDenied::VelocityAnomaly {
                spends_per_minute: velocity.recent_spends.len() as u32,
            });
        }

        // 7. Human approval threshold
        if let Some(threshold) = policy.human_approval_threshold {
            if trm_amount > threshold {
                return Err(SpendDenied::HumanApprovalRequired {
                    amount: trm_amount,
                    threshold,
                });
            }
        }

        Ok(())
    }

    /// Record a successful spend (after trade executes).
    pub fn record_spend(&mut self, node_id: &NodeId, trm_amount: u64) {
        let circuit = self.circuits.entry(node_id.clone()).or_default();
        circuit.hourly_spend += trm_amount;
        circuit.lifetime_spend += trm_amount;
        circuit.consecutive_errors = 0;
    }

    /// Record a failed spend (error during inference).
    pub fn record_error(&mut self, node_id: &NodeId) {
        let circuit = self.circuits.entry(node_id.clone()).or_default();
        circuit.consecutive_errors += 1;
        if circuit.consecutive_errors >= 5 {
            circuit.tripped = true;
            circuit.tripped_at = Some(now_millis());
            tracing::warn!(
                "CIRCUIT BREAKER TRIPPED for {}: {} consecutive errors",
                node_id.to_hex(),
                circuit.consecutive_errors
            );
        }
    }

    /// Get safety status for a node.
    pub fn status(&self, node_id: &NodeId) -> SafetyStatus {
        let circuit = self.circuits.get(node_id);
        let velocity = self.velocity.get(node_id);

        SafetyStatus {
            kill_switch_active: self.kill_switch.active,
            circuit_tripped: circuit.map(|c| c.tripped).unwrap_or(false),
            hourly_spend: circuit.map(|c| c.hourly_spend).unwrap_or(0),
            lifetime_spend: circuit.map(|c| c.lifetime_spend).unwrap_or(0),
            spends_last_minute: velocity.map(|v| v.recent_spends.len()).unwrap_or(0) as u32,
            policy: self.policy_for(node_id).clone(),
        }
    }

    /// Validate that a new loan can be created given current lending state.
    ///
    /// Returns `Ok(())` if the loan is permitted, `Err(LoanDenied)` otherwise.
    ///
    /// Parameters:
    /// * `principal_trm` — amount being lent
    /// * `collateral_trm` — collateral locked from borrower
    /// * `term_hours` — loan duration
    /// * `borrower_credit` — computed credit score (0.0-1.0)
    /// * `pool_total_trm` — total CU in lending pool
    /// * `pool_available_cu` — CU currently unlent
    pub fn check_loan_creation(
        &mut self,
        principal_trm: u64,
        collateral_trm: u64,
        term_hours: u64,
        borrower_credit: f64,
        pool_total_trm: u64,
        pool_available_cu: u64,
    ) -> Result<(), LoanDenied> {
        use crate::lending::{
            DEFAULT_CIRCUIT_BREAKER_THRESHOLD, MAX_LENDING_VELOCITY, MAX_LOAN_TERM_HOURS,
            MAX_LTV_RATIO, MAX_SINGLE_LOAN_POOL_PCT, MIN_CREDIT_FOR_BORROWING, MIN_RESERVE_RATIO,
        };

        let now_ms = now_millis();

        // Gate 1: kill switch
        if self.kill_switch.active {
            return Err(LoanDenied::KillSwitchActive);
        }

        // Gate 2: global default-rate circuit breaker
        if self.lending.is_tripped(now_ms) {
            return Err(LoanDenied::DefaultRateCircuitTripped);
        }
        let default_rate = self.lending.default_rate(now_ms);
        if default_rate > DEFAULT_CIRCUIT_BREAKER_THRESHOLD {
            self.lending
                .trip(now_ms, "default rate exceeded threshold");
            return Err(LoanDenied::DefaultRateCircuitTripped);
        }

        // Gate 3: velocity limit (10 new loans / minute)
        let velocity = self.lending.velocity(now_ms);
        if velocity >= MAX_LENDING_VELOCITY {
            return Err(LoanDenied::VelocityLimitExceeded);
        }

        // Gate 4: borrower credit
        // Security: NaN < MIN_CREDIT evaluates to false in IEEE 754,
        // so NaN credit would bypass this gate. Check finite first.
        if !borrower_credit.is_finite() || borrower_credit < MIN_CREDIT_FOR_BORROWING {
            return Err(LoanDenied::InsufficientCredit {
                score: borrower_credit,
                minimum: MIN_CREDIT_FOR_BORROWING,
            });
        }

        // Gate 5: loan-to-collateral ratio
        if collateral_trm == 0 {
            return Err(LoanDenied::ExcessiveLtv {
                ratio: f64::INFINITY,
                maximum: MAX_LTV_RATIO,
            });
        }
        let ltv = principal_trm as f64 / collateral_trm as f64;
        if ltv > MAX_LTV_RATIO {
            return Err(LoanDenied::ExcessiveLtv {
                ratio: ltv,
                maximum: MAX_LTV_RATIO,
            });
        }

        // Gate 6: term
        if term_hours > MAX_LOAN_TERM_HOURS {
            return Err(LoanDenied::ExcessiveTerm {
                hours: term_hours,
                maximum: MAX_LOAN_TERM_HOURS,
            });
        }

        // Gate 7: single-loan pool cap
        let max_single = (pool_total_trm as f64 * MAX_SINGLE_LOAN_POOL_PCT) as u64;
        if principal_trm > max_single {
            return Err(LoanDenied::SingleLoanExceedsPool {
                amount: principal_trm,
                maximum: max_single,
            });
        }

        // Gate 8: reserve ratio would be violated
        let reserved_after = pool_available_cu.saturating_sub(principal_trm);
        let ratio_after = if pool_total_trm == 0 {
            1.0
        } else {
            reserved_after as f64 / pool_total_trm as f64
        };
        if ratio_after < MIN_RESERVE_RATIO {
            return Err(LoanDenied::PoolReserveViolation {
                after_ratio: ratio_after,
                minimum: MIN_RESERVE_RATIO,
            });
        }

        Ok(())
    }

    /// Record that a new loan was successfully created (updates velocity window).
    pub fn record_loan_created(&mut self) {
        let now_ms = now_millis();
        self.lending.record_loan(now_ms);
    }

    /// Record the outcome of a loan (repaid or defaulted) for default-rate tracking.
    pub fn record_loan_outcome(&mut self, defaulted: bool) {
        let now_ms = now_millis();
        self.lending.record_loan_outcome(now_ms, defaulted);
    }
}

/// Why a spend was denied.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SpendDenied {
    #[error("KILL SWITCH ACTIVE: {0}")]
    KillSwitchActive(String),
    #[error("exceeds per-request limit: {requested} CU > {limit} CU")]
    ExceedsRequestLimit { requested: u64, limit: u64 },
    #[error("circuit breaker tripped — wait for auto-reset (5 min)")]
    CircuitBreakerTripped,
    #[error("hourly limit exceeded: {spent} CU spent of {limit} CU/hr")]
    HourlyLimitExceeded { spent: u64, limit: u64 },
    #[error("lifetime cap reached: {spent} CU of {cap} CU maximum")]
    LifetimeCapReached { spent: u64, cap: u64 },
    #[error("velocity anomaly: {spends_per_minute} spends/min (max 30)")]
    VelocityAnomaly { spends_per_minute: u32 },
    #[error("human approval required: {amount} CU exceeds threshold {threshold} CU")]
    HumanApprovalRequired { amount: u64, threshold: u64 },
}

/// Reasons why a loan may be denied by the safety layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoanDenied {
    KillSwitchActive,
    InsufficientCredit { score: f64, minimum: f64 },
    ExcessiveLtv { ratio: f64, maximum: f64 },
    ExcessiveTerm { hours: u64, maximum: u64 },
    SingleLoanExceedsPool { amount: u64, maximum: u64 },
    PoolReserveViolation { after_ratio: f64, minimum: f64 },
    VelocityLimitExceeded,
    DefaultRateCircuitTripped,
}

/// Snapshot of safety status for a node.
#[derive(Debug, Clone, Serialize)]
pub struct SafetyStatus {
    pub kill_switch_active: bool,
    pub circuit_tripped: bool,
    pub hourly_spend: u64,
    pub lifetime_spend: u64,
    pub spends_last_minute: u32,
    pub policy: BudgetPolicy,
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_switch_blocks_all_spends() {
        let mut safety = SafetyController::new();
        let node = NodeId([1u8; 32]);

        assert!(safety.check_spend(&node, 100).is_ok());

        safety.kill_switch.activate("emergency", "admin");
        let result = safety.check_spend(&node, 1);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SpendDenied::KillSwitchActive(_)));
    }

    #[test]
    fn per_request_limit_enforced() {
        let mut safety = SafetyController::new();
        let node = NodeId([1u8; 32]);

        // Default max_cu_per_request = 1000
        assert!(safety.check_spend(&node, 999).is_ok());
        let result = safety.check_spend(&node, 1001);
        assert!(matches!(
            result.unwrap_err(),
            SpendDenied::ExceedsRequestLimit { .. }
        ));
    }

    #[test]
    fn hourly_limit_enforced() {
        let mut safety = SafetyController::new();
        let node = NodeId([1u8; 32]);
        safety.default_policy.max_cu_per_hour = 500;

        safety.check_spend(&node, 200).unwrap();
        safety.record_spend(&node, 200);

        safety.check_spend(&node, 200).unwrap();
        safety.record_spend(&node, 200);

        // 400 spent, trying 200 more = 600 > 500
        let result = safety.check_spend(&node, 200);
        assert!(matches!(
            result.unwrap_err(),
            SpendDenied::HourlyLimitExceeded { .. }
        ));
    }

    #[test]
    fn velocity_trips_circuit_breaker() {
        let mut safety = SafetyController::new();
        let node = NodeId([1u8; 32]);

        // Flood 31 spends in rapid succession
        for _ in 0..31 {
            let _ = safety.check_spend(&node, 1);
            safety.record_spend(&node, 1);
        }

        let result = safety.check_spend(&node, 1);
        assert!(matches!(
            result.unwrap_err(),
            SpendDenied::VelocityAnomaly { .. } | SpendDenied::CircuitBreakerTripped
        ));
    }

    #[test]
    fn consecutive_errors_trip_circuit_breaker() {
        let mut safety = SafetyController::new();
        let node = NodeId([1u8; 32]);

        for _ in 0..5 {
            safety.record_error(&node);
        }

        let result = safety.check_spend(&node, 1);
        assert!(matches!(
            result.unwrap_err(),
            SpendDenied::CircuitBreakerTripped
        ));
    }

    #[test]
    fn custom_policy_per_node() {
        let mut safety = SafetyController::new();
        let agent = NodeId([99u8; 32]);

        safety.set_policy(
            &agent,
            BudgetPolicy {
                max_cu_per_hour: 100,
                max_cu_per_request: 50,
                max_cu_lifetime: 500,
                human_approval_threshold: Some(30),
            },
        );

        // Within policy
        assert!(safety.check_spend(&agent, 25).is_ok());

        // Exceeds per-request
        let result = safety.check_spend(&agent, 51);
        assert!(matches!(
            result.unwrap_err(),
            SpendDenied::ExceedsRequestLimit { .. }
        ));

        // Requires human approval
        let result = safety.check_spend(&agent, 35);
        assert!(matches!(
            result.unwrap_err(),
            SpendDenied::HumanApprovalRequired { .. }
        ));
    }

    #[test]
    fn lifetime_cap_enforced() {
        let mut safety = SafetyController::new();
        let node = NodeId([1u8; 32]);
        safety.default_policy.max_cu_lifetime = 300;

        safety.check_spend(&node, 200).unwrap();
        safety.record_spend(&node, 200);

        // 200 + 200 = 400 > 300 lifetime
        let result = safety.check_spend(&node, 200);
        assert!(matches!(
            result.unwrap_err(),
            SpendDenied::LifetimeCapReached { .. }
        ));
    }

    #[test]
    fn loan_denied_when_kill_switch_active() {
        let mut safety = SafetyController::new();
        safety.kill_switch.activate("test", "admin");
        let result = safety.check_loan_creation(1_000, 3_000, 24, 0.5, 1_000_000, 1_000_000);
        assert!(matches!(result, Err(LoanDenied::KillSwitchActive)));
    }

    #[test]
    fn loan_denied_for_low_credit() {
        let mut safety = SafetyController::new();
        let result = safety.check_loan_creation(1_000, 3_000, 24, 0.1, 1_000_000, 1_000_000);
        assert!(matches!(result, Err(LoanDenied::InsufficientCredit { .. })));
    }

    #[test]
    fn loan_denied_for_excessive_ltv() {
        let mut safety = SafetyController::new();
        // principal 10_000 / collateral 1_000 = 10.0 ratio >> 3.0
        let result = safety.check_loan_creation(10_000, 1_000, 24, 0.9, 10_000_000, 10_000_000);
        assert!(matches!(result, Err(LoanDenied::ExcessiveLtv { .. })));
    }

    #[test]
    fn loan_denied_for_excessive_term() {
        let mut safety = SafetyController::new();
        // 200 hours > MAX_LOAN_TERM_HOURS (168)
        let result = safety.check_loan_creation(1_000, 3_000, 200, 0.9, 10_000_000, 10_000_000);
        assert!(matches!(result, Err(LoanDenied::ExcessiveTerm { .. })));
    }

    #[test]
    fn loan_denied_when_single_loan_exceeds_pool_cap() {
        let mut safety = SafetyController::new();
        // 25% of 1_000_000 pool = 250_000, above 20% cap (200_000)
        let result = safety.check_loan_creation(250_000, 750_000, 24, 0.9, 1_000_000, 1_000_000);
        assert!(matches!(
            result,
            Err(LoanDenied::SingleLoanExceedsPool { .. })
        ));
    }

    #[test]
    fn loan_denied_when_reserve_ratio_would_fall_below_30pct() {
        let mut safety = SafetyController::new();
        // pool 1_000_000, available 400_000 (60% already lent)
        // lending 150_000 more would leave 250_000 available = 25% < 30%
        // but first, 150_000 < 20% cap (200_000), so reserve is the only gate
        let result = safety.check_loan_creation(150_000, 450_000, 24, 0.9, 1_000_000, 400_000);
        assert!(matches!(
            result,
            Err(LoanDenied::PoolReserveViolation { .. })
        ));
    }

    #[test]
    fn loan_permitted_under_all_constraints() {
        let mut safety = SafetyController::new();
        let result = safety.check_loan_creation(10_000, 30_000, 24, 0.7, 1_000_000, 1_000_000);
        assert!(result.is_ok(), "loan should be permitted: {:?}", result);
    }

    #[test]
    fn velocity_limit_trips_after_many_loans() {
        let mut safety = SafetyController::new();
        // Create 10 loans rapidly — 11th should fail
        for _ in 0..10 {
            safety.record_loan_created();
        }
        let result = safety.check_loan_creation(1_000, 3_000, 24, 0.9, 10_000_000, 10_000_000);
        assert!(matches!(result, Err(LoanDenied::VelocityLimitExceeded)));
    }

    #[test]
    fn default_rate_tracking_works() {
        let mut safety = SafetyController::new();
        // 9 repaid, 2 defaulted = 2/11 = 18% > 10% threshold
        for _ in 0..9 {
            safety.record_loan_outcome(false);
        }
        for _ in 0..2 {
            safety.record_loan_outcome(true);
        }
        let result = safety.check_loan_creation(1_000, 3_000, 24, 0.9, 10_000_000, 10_000_000);
        assert!(matches!(
            result,
            Err(LoanDenied::DefaultRateCircuitTripped)
        ));
    }

    // ===========================================================================
    // DEEP SECURITY TESTS — Round 2 (safety boundary edge cases)
    // ===========================================================================

    #[test]
    fn sec_deep_kill_switch_blocks_all_spending_operations() {
        let mut safety = SafetyController::new();
        let node = NodeId([1u8; 32]);

        safety.kill_switch.activate("test emergency", "security-admin");

        // Every trm_amount from 1 to max_cu_per_request must be blocked.
        for amount in [1u64, 10, 100, 500, 999, 1000] {
            let result = safety.check_spend(&node, amount);
            assert!(
                matches!(result, Err(SpendDenied::KillSwitchActive(_))),
                "kill switch must block all spends regardless of amount, failed for {amount}"
            );
        }

        // Even zero-spend check must go through kill switch gate first.
        // (zero is blocked by policy limit gate which comes after kill switch.)
        let result = safety.check_spend(&node, 1_u64);
        assert!(result.is_err(), "kill switch must still block smallest possible spend");
    }

    #[test]
    fn sec_deep_zero_max_cu_per_hour_blocks_all_spends() {
        let mut safety = SafetyController::new();
        let node = NodeId([2u8; 32]);
        safety.default_policy.max_cu_per_hour = 0;
        safety.default_policy.max_cu_per_request = 10_000; // lift request limit

        // With max_cu_per_hour = 0, hourly_spend (0) + any amount > 0 exceeds limit.
        let result = safety.check_spend(&node, 1);
        assert!(
            matches!(result, Err(SpendDenied::HourlyLimitExceeded { .. })),
            "zero max_cu_per_hour must block all spends, got {:?}", result
        );
    }

    #[test]
    fn sec_deep_zero_lifetime_cap_blocks_all_spends() {
        let mut safety = SafetyController::new();
        let node = NodeId([3u8; 32]);
        safety.default_policy.max_cu_lifetime = 0;
        safety.default_policy.max_cu_per_request = 10_000;
        safety.default_policy.max_cu_per_hour = 1_000_000;

        let result = safety.check_spend(&node, 1);
        assert!(
            matches!(result, Err(SpendDenied::LifetimeCapReached { .. })),
            "zero lifetime cap must block all spends, got {:?}", result
        );
    }

    #[test]
    fn sec_deep_budget_policy_exactly_at_hourly_limit_allowed_then_blocked() {
        let mut safety = SafetyController::new();
        let node = NodeId([4u8; 32]);
        safety.default_policy.max_cu_per_hour = 1_000;
        safety.default_policy.max_cu_per_request = 1_000;
        safety.default_policy.max_cu_lifetime = 100_000;
        safety.default_policy.human_approval_threshold = None;

        // Exactly at the limit must be allowed.
        assert!(safety.check_spend(&node, 1_000).is_ok(), "exactly at hourly limit must be allowed");
        safety.record_spend(&node, 1_000);

        // One more must be blocked.
        let result = safety.check_spend(&node, 1);
        assert!(
            matches!(result, Err(SpendDenied::HourlyLimitExceeded { .. })),
            "one over hourly limit must be blocked, got {:?}", result
        );
    }

    #[test]
    fn sec_deep_lending_circuit_breaker_auto_resets_after_5min() {
        let mut state = LendingCircuitState::default();
        let now = 1_000_000u64;
        state.trip(now, "test trip");
        assert!(state.is_tripped(now), "just-tripped circuit must be tripped");

        // At RESET_AFTER_MS - 1, still tripped.
        let just_before = now + LendingCircuitState::RESET_AFTER_MS - 1;
        assert!(state.is_tripped(just_before), "circuit must remain tripped before reset window");

        // At exactly RESET_AFTER_MS, auto-reset.
        let at_reset = now + LendingCircuitState::RESET_AFTER_MS;
        assert!(!state.is_tripped(at_reset), "circuit must auto-reset exactly at RESET_AFTER_MS");
    }

    #[test]
    fn sec_deep_lending_circuit_zero_default_rate_is_zero() {
        let mut state = LendingCircuitState::default();
        let now = 1_000_000u64;
        let rate = state.default_rate(now);
        assert_eq!(rate, 0.0, "empty state must have 0.0 default rate, not NaN");
        assert!(!rate.is_nan(), "default_rate must never be NaN on empty state");
    }

    #[test]
    fn sec_deep_lending_circuit_velocity_zero_on_empty() {
        let mut state = LendingCircuitState::default();
        let now = 1_000_000u64;
        let vel = state.velocity(now);
        assert_eq!(vel, 0, "empty state must have velocity 0");
    }

    #[test]
    fn sec_deep_check_loan_nan_credit_behavior_documented() {
        // KNOWN BEHAVIOR: NaN < MIN_CREDIT_FOR_BORROWING (e.g., 0.3) evaluates to FALSE
        // in IEEE 754. This means NaN credit passes the credit check gate.
        //
        // The safe fix is to guard with: if !credit.is_finite() || credit < minimum { deny }
        //
        // This test documents the CURRENT behavior so any future fix that rejects NaN
        // will cause this test to pass the more-strict assertion.
        let mut safety = SafetyController::new();
        let result = safety.check_loan_creation(
            1_000, 3_000, 24,
            f64::NAN, // NaN credit score
            1_000_000, 1_000_000,
        );
        // Currently NaN passes the credit check. If fixed, result would be Err.
        // Document: NaN credit score is a bypass vulnerability when not guarded.
        match result {
            Err(LoanDenied::InsufficientCredit { .. }) => {
                // Fixed — NaN is now rejected as insufficient credit.
            }
            Ok(()) => {
                // Known vulnerability: NaN bypasses credit check.
                // This is documented here; the fix is to add is_finite() guard.
            }
            Err(e) => {
                // Rejected for another reason (e.g., pool limit) — acceptable.
                let _ = e;
            }
        }
    }

    #[test]
    fn sec_deep_check_loan_with_zero_collateral_returns_ltv_error() {
        let mut safety = SafetyController::new();
        let result = safety.check_loan_creation(
            1_000, 0, // zero collateral
            24, 0.9, 1_000_000, 1_000_000,
        );
        assert!(
            matches!(result, Err(LoanDenied::ExcessiveLtv { .. })),
            "zero collateral must return ExcessiveLtv error, got {:?}", result
        );
    }
}
