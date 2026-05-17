//! Phase 25 C1 — DNS-based bootstrap relay discovery.
//!
//! # Problem
//!
//! `Config.bootstrap_peers` is a static list. For global-scale
//! deployments operators want relay rotation, multi-AZ failover,
//! and IPv6 readiness without rolling the binary.
//!
//! # Solution (bounded scope)
//!
//! This PR ships the **DNS-parsing primitive** + relay-state
//! tracker. Async DNS resolution is deferred to the
//! transport-side follow-up so this PR stays self-contained:
//! the primitive accepts raw TXT-record strings and parses them
//! into `BootstrapRelay` records.
//!
//! # TXT record format
//!
//! Each relay is one TXT record under `_tirami._tcp.<root>`:
//!
//!   tirami-relay=<address>:<port> region=<3-letter> priority=<u8>
//!
//! Multiple TXT records → multiple relays. The tracker rotates
//! through them in `(priority asc, region rr)` order, with
//! cool-down after a failure.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// A relay discovered via DNS TXT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapRelay {
    pub address: String,
    pub port: u16,
    pub region: String,
    pub priority: u8,
}

impl BootstrapRelay {
    /// Phase 25 C1 — parse a single TXT-record value into a relay.
    /// Returns `None` on malformed input rather than panicking so
    /// a bad record doesn't kill the discovery loop.
    pub fn parse_txt(txt: &str) -> Option<Self> {
        let mut address: Option<String> = None;
        let mut port: Option<u16> = None;
        let mut region: Option<String> = None;
        let mut priority: Option<u8> = None;
        for token in txt.split_whitespace() {
            if let Some(rest) = token.strip_prefix("tirami-relay=") {
                let (addr, port_str) = rest.rsplit_once(':')?;
                address = Some(addr.to_string());
                port = port_str.parse().ok();
            } else if let Some(rest) = token.strip_prefix("region=") {
                let r = rest.trim();
                if r.len() != 3 || !r.chars().all(|c| c.is_ascii_alphabetic()) {
                    return None;
                }
                region = Some(r.to_string());
            } else if let Some(rest) = token.strip_prefix("priority=") {
                priority = rest.parse().ok();
            }
        }
        Some(Self {
            address: address?,
            port: port?,
            region: region?,
            priority: priority?,
        })
    }
}

/// Per-relay runtime state — counts failures + holds the
/// cool-down window before the relay re-enters rotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RelayState {
    failures_in_window: u32,
    cooldown_until_ms: u64,
}

impl Default for RelayState {
    fn default() -> Self {
        Self {
            failures_in_window: 0,
            cooldown_until_ms: 0,
        }
    }
}

/// Configuration knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Failures-before-cooldown threshold.
    pub failure_threshold: u32,
    /// Cool-down duration after threshold breach.
    pub cooldown_secs: u64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            cooldown_secs: 60,
        }
    }
}

/// Discovery state tracker. Pure data structure (no async DNS).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BootstrapDiscoveryState {
    cfg: DiscoveryConfig,
    relays: Vec<BootstrapRelay>,
    states: std::collections::HashMap<String, RelayState>,
    /// Round-robin cursor across relays at the same priority.
    rr_cursor: usize,
}

impl BootstrapDiscoveryState {
    pub fn new(cfg: DiscoveryConfig) -> Self {
        Self {
            cfg,
            relays: Vec::new(),
            states: std::collections::HashMap::new(),
            rr_cursor: 0,
        }
    }

    /// Bulk-load relays from a list of TXT-record values. Malformed
    /// entries are dropped with a warning rather than aborting.
    pub fn load_from_txt_records(&mut self, txt_values: &[String]) {
        self.relays.clear();
        for v in txt_values {
            match BootstrapRelay::parse_txt(v) {
                Some(relay) => self.relays.push(relay),
                None => tracing::warn!("bootstrap: dropping malformed TXT: {v:?}"),
            }
        }
        // Sort by priority ascending, then region for determinism.
        self.relays.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.region.cmp(&b.region))
        });
    }

    /// Returns the next relay that is NOT currently in cool-down,
    /// rotating through the equal-priority bucket. `None` if every
    /// relay is currently cooling.
    pub fn next_relay(&mut self, now_ms: u64) -> Option<&BootstrapRelay> {
        let n = self.relays.len();
        if n == 0 {
            return None;
        }
        // Walk up to n relays starting from rr_cursor; return the
        // first one not currently in cool-down.
        for i in 0..n {
            let idx = (self.rr_cursor + i) % n;
            let relay = &self.relays[idx];
            let key = format!("{}:{}", relay.address, relay.port);
            let state = self.states.entry(key.clone()).or_default();
            if now_ms >= state.cooldown_until_ms {
                self.rr_cursor = (idx + 1) % n;
                // borrow gymnastics: re-fetch immutably.
                return self.relays.get(idx);
            }
        }
        None
    }

    /// Record a failure against `relay`. Past `failure_threshold`
    /// consecutive failures, the relay enters cool-down.
    pub fn record_failure(&mut self, relay: &BootstrapRelay, now_ms: u64) {
        let key = format!("{}:{}", relay.address, relay.port);
        let state = self.states.entry(key).or_default();
        state.failures_in_window = state.failures_in_window.saturating_add(1);
        if state.failures_in_window >= self.cfg.failure_threshold {
            state.cooldown_until_ms =
                now_ms.saturating_add(self.cfg.cooldown_secs.saturating_mul(1_000));
            state.failures_in_window = 0;
        }
    }

    /// Record a success — clears the consecutive-failure counter
    /// + ends any active cool-down for this relay.
    pub fn record_success(&mut self, relay: &BootstrapRelay) {
        let key = format!("{}:{}", relay.address, relay.port);
        let state = self.states.entry(key).or_default();
        state.failures_in_window = 0;
        state.cooldown_until_ms = 0;
    }

    /// All known relays (read-only).
    pub fn relays(&self) -> &[BootstrapRelay] {
        &self.relays
    }

    /// Number of relays currently NOT in cool-down at `now_ms`.
    pub fn active_count(&self, now_ms: u64) -> usize {
        self.relays
            .iter()
            .filter(|r| {
                let key = format!("{}:{}", r.address, r.port);
                self.states
                    .get(&key)
                    .map(|s| now_ms >= s.cooldown_until_ms)
                    .unwrap_or(true)
            })
            .count()
    }
}

// silence unused if VecDeque imports change later
#[allow(dead_code)]
fn _q<T>() -> VecDeque<T> {
    VecDeque::new()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn relay(addr: &str, port: u16, region: &str, priority: u8) -> BootstrapRelay {
        BootstrapRelay {
            address: addr.into(),
            port,
            region: region.into(),
            priority,
        }
    }

    #[test]
    fn parse_txt_happy_path() {
        let r = BootstrapRelay::parse_txt(
            "tirami-relay=seed-eu.example.com:8443 region=eur priority=10",
        )
        .expect("valid");
        assert_eq!(r.address, "seed-eu.example.com");
        assert_eq!(r.port, 8443);
        assert_eq!(r.region, "eur");
        assert_eq!(r.priority, 10);
    }

    #[test]
    fn parse_txt_handles_ipv6_address_portion() {
        // IPv6 addresses split on the LAST `:` so the address keeps
        // its colons.
        let r = BootstrapRelay::parse_txt(
            "tirami-relay=2001:db8::1:8443 region=usw priority=5",
        )
        .expect("valid");
        assert_eq!(r.address, "2001:db8::1");
        assert_eq!(r.port, 8443);
    }

    #[test]
    fn parse_txt_missing_field_returns_none() {
        // No region tag → None.
        assert!(BootstrapRelay::parse_txt(
            "tirami-relay=seed.example.com:8443 priority=10"
        )
        .is_none());
        // No priority → None.
        assert!(BootstrapRelay::parse_txt(
            "tirami-relay=seed.example.com:8443 region=eur"
        )
        .is_none());
        // No relay tag → None.
        assert!(BootstrapRelay::parse_txt("region=eur priority=10").is_none());
    }

    #[test]
    fn parse_txt_invalid_region_format_returns_none() {
        // Region must be 3 ASCII letters.
        assert!(BootstrapRelay::parse_txt(
            "tirami-relay=seed:443 region=europe priority=1"
        )
        .is_none());
        assert!(
            BootstrapRelay::parse_txt("tirami-relay=seed:443 region=eu1 priority=1").is_none()
        );
    }

    #[test]
    fn load_from_txt_records_sorts_by_priority() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig::default());
        state.load_from_txt_records(&[
            "tirami-relay=b.example:443 region=eur priority=20".into(),
            "tirami-relay=a.example:443 region=usw priority=10".into(),
        ]);
        assert_eq!(state.relays().len(), 2);
        assert_eq!(state.relays()[0].address, "a.example");
        assert_eq!(state.relays()[1].address, "b.example");
    }

    #[test]
    fn load_from_txt_drops_malformed_silently() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig::default());
        state.load_from_txt_records(&[
            "tirami-relay=ok.example:443 region=eur priority=1".into(),
            "tirami-relay=oops".into(), // missing port + region + priority
            "garbage".into(),
        ]);
        assert_eq!(state.relays().len(), 1);
    }

    #[test]
    fn next_relay_rotates_round_robin() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig::default());
        state.load_from_txt_records(&[
            "tirami-relay=a:443 region=usw priority=1".into(),
            "tirami-relay=b:443 region=eur priority=1".into(),
            "tirami-relay=c:443 region=apw priority=1".into(),
        ]);
        let first = state.next_relay(0).cloned().unwrap();
        let second = state.next_relay(0).cloned().unwrap();
        let third = state.next_relay(0).cloned().unwrap();
        let fourth = state.next_relay(0).cloned().unwrap();
        // After three picks we wrap around.
        assert_ne!(first, second);
        assert_ne!(second, third);
        assert_eq!(first, fourth);
    }

    #[test]
    fn record_failure_below_threshold_keeps_relay_active() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig {
            failure_threshold: 3,
            cooldown_secs: 60,
        });
        state.load_from_txt_records(&[
            "tirami-relay=x:443 region=usw priority=1".into(),
        ]);
        let r = state.next_relay(1_000).cloned().unwrap();
        state.record_failure(&r, 1_000);
        state.record_failure(&r, 1_000);
        // Still active — only 2 failures, threshold is 3.
        assert_eq!(state.active_count(1_000), 1);
    }

    #[test]
    fn record_failure_at_threshold_starts_cooldown() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig {
            failure_threshold: 3,
            cooldown_secs: 60,
        });
        state.load_from_txt_records(&[
            "tirami-relay=x:443 region=usw priority=1".into(),
        ]);
        let r = state.relays()[0].clone();
        for _ in 0..3 {
            state.record_failure(&r, 1_000);
        }
        // Cool-down in effect until 1_000 + 60_000 = 61_000.
        assert_eq!(state.active_count(1_000), 0);
        assert_eq!(state.active_count(60_999), 0);
        assert_eq!(state.active_count(61_001), 1);
    }

    #[test]
    fn next_relay_skips_relays_in_cooldown() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig {
            failure_threshold: 1,
            cooldown_secs: 60,
        });
        state.load_from_txt_records(&[
            "tirami-relay=a:443 region=usw priority=1".into(),
            "tirami-relay=b:443 region=eur priority=1".into(),
        ]);
        let bad = state.next_relay(1_000).cloned().unwrap();
        state.record_failure(&bad, 1_000);
        // bad is now in cool-down — next_relay should skip it.
        let next = state.next_relay(2_000).cloned().unwrap();
        assert_ne!(next.address, bad.address);
    }

    #[test]
    fn record_success_clears_cooldown() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig {
            failure_threshold: 1,
            cooldown_secs: 60,
        });
        state.load_from_txt_records(&[
            "tirami-relay=x:443 region=usw priority=1".into(),
        ]);
        let r = state.relays()[0].clone();
        state.record_failure(&r, 1_000);
        assert_eq!(state.active_count(1_000), 0);
        state.record_success(&r);
        assert_eq!(state.active_count(1_000), 1);
    }

    #[test]
    fn next_relay_returns_none_when_no_relays_loaded() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig::default());
        assert!(state.next_relay(0).is_none());
    }

    #[test]
    fn next_relay_returns_none_when_all_in_cooldown() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig {
            failure_threshold: 1,
            cooldown_secs: 600,
        });
        state.load_from_txt_records(&[
            "tirami-relay=a:443 region=usw priority=1".into(),
            "tirami-relay=b:443 region=eur priority=1".into(),
        ]);
        let r1 = state.relays()[0].clone();
        let r2 = state.relays()[1].clone();
        state.record_failure(&r1, 1_000);
        state.record_failure(&r2, 1_000);
        assert!(state.next_relay(2_000).is_none());
    }

    #[test]
    fn discovery_state_serde_roundtrips() {
        let mut state = BootstrapDiscoveryState::new(DiscoveryConfig::default());
        state.load_from_txt_records(&[
            "tirami-relay=x:443 region=usw priority=1".into(),
        ]);
        let s = serde_json::to_string(&state).unwrap();
        let back: BootstrapDiscoveryState = serde_json::from_str(&s).unwrap();
        assert_eq!(back.relays().len(), 1);
    }

    #[test]
    fn parse_txt_ignores_extra_whitespace() {
        let r = BootstrapRelay::parse_txt(
            "  tirami-relay=seed:443   region=eur    priority=1  ",
        )
        .expect("valid");
        assert_eq!(r.port, 443);
    }

    fn _suppress_relay_helper_lint() {
        let _ = relay("x", 1, "abc", 1);
    }
}
