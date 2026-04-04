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

use forge_core::NodeId;
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
    recent_spends: Vec<(u64, u64)>, // (timestamp, cu_amount)
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
        cu_amount: u64,
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
        if cu_amount > policy.max_cu_per_request {
            return Err(SpendDenied::ExceedsRequestLimit {
                requested: cu_amount,
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
        if circuit.hourly_spend + cu_amount > policy.max_cu_per_hour {
            return Err(SpendDenied::HourlyLimitExceeded {
                spent: circuit.hourly_spend,
                limit: policy.max_cu_per_hour,
            });
        }

        // 5. Lifetime cap
        if circuit.lifetime_spend + cu_amount > policy.max_cu_lifetime {
            return Err(SpendDenied::LifetimeCapReached {
                spent: circuit.lifetime_spend,
                cap: policy.max_cu_lifetime,
            });
        }

        // 6. Spend velocity check
        let velocity = self.velocity.entry(node_id.clone()).or_default();
        velocity.record(cu_amount);
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
            if cu_amount > threshold {
                return Err(SpendDenied::HumanApprovalRequired {
                    amount: cu_amount,
                    threshold,
                });
            }
        }

        Ok(())
    }

    /// Record a successful spend (after trade executes).
    pub fn record_spend(&mut self, node_id: &NodeId, cu_amount: u64) {
        let circuit = self.circuits.entry(node_id.clone()).or_default();
        circuit.hourly_spend += cu_amount;
        circuit.lifetime_spend += cu_amount;
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
}
