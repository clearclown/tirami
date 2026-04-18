//! Phase 17 Wave 2.8 — strengthened Sybil defense for welcome loans.
//!
//! # Problem
//!
//! The pre-Phase-17 welcome-loan gate was a single local counter:
//! "reject if I locally know more than 100 nodes with `contributed == 0`".
//! An attacker using one cloud provider can trivially rotate through
//! thousands of IPs inside a single ASN and collect 1 000 TRM ×
//! threshold-worth of interest-free loans.
//!
//! # Solution
//!
//! This module delivers two strands of hardening, both opt-in so
//! operators without an IP→ASN resolver (Wave 2.3) aren't locked out:
//!
//! 1. **Per-bucket rolling window**: [`WelcomeLoanLimiter`] records
//!    each welcome-loan grant against a caller-supplied "bucket key"
//!    (typically ASN, or a subnet prefix, or country code). It caps
//!    grants at `max_per_bucket_per_window` over a sliding
//!    24-hour window. Beyond the cap → reject.
//! 2. **Stake-proof multiplier**: callers can pass
//!    `stake_proven = true` when the requester has demonstrated
//!    possession of a staked L2 address (Wave 2.7 lands the actual
//!    verification). Stake-proven peers get
//!    [`STAKED_THRESHOLD_MULTIPLIER`]× the per-bucket cap, so a
//!    legitimate cloud operator can still onboard many nodes.
//!
//! # Contract with ComputeLedger
//!
//! The limiter is a separate primitive, not owned by `ComputeLedger`,
//! for two reasons:
//! * ASN resolution lives in `tirami-net` (the network layer holds
//!   the peer's IP); bolting it into the ledger would import the
//!   whole net stack.
//! * Different deployments may bucket by different dimensions
//!   (ASN, subnet, GeoIP country). Keeping the key type
//!   `String` lets operators pick without recompilation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Tuning knobs
// ---------------------------------------------------------------------------

/// Default rolling window, in milliseconds. 24 hours.
pub const DEFAULT_WELCOME_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;

/// Default cap: 10 welcome loans per bucket per 24-hour window.
/// Picked to comfortably allow a small legitimate operator onboarding
/// a handful of nodes per day, while stopping a Sybil flood dead.
pub const DEFAULT_MAX_PER_BUCKET_PER_WINDOW: usize = 10;

/// Multiplier applied to the per-bucket cap when the caller has
/// proven L2-address stake. Wave 2.7 land the real proof path; until
/// then the multiplier is wired but the `stake_proven` flag must be
/// determined out of band.
pub const STAKED_THRESHOLD_MULTIPLIER: usize = 10;

// ---------------------------------------------------------------------------
// WelcomeLoanLimiter
// ---------------------------------------------------------------------------

/// Configuration for [`WelcomeLoanLimiter`]. Persistable so operators
/// can tune via config without recompiling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WelcomeLoanLimiterConfig {
    /// Rolling window length in milliseconds.
    pub window_ms: u64,
    /// Max grants per bucket within the window when the caller is
    /// NOT stake-proven.
    pub max_per_bucket_per_window: usize,
    /// Multiplier applied to the cap when `stake_proven = true`.
    pub staked_multiplier: usize,
}

impl Default for WelcomeLoanLimiterConfig {
    fn default() -> Self {
        Self {
            window_ms: DEFAULT_WELCOME_WINDOW_MS,
            max_per_bucket_per_window: DEFAULT_MAX_PER_BUCKET_PER_WINDOW,
            staked_multiplier: STAKED_THRESHOLD_MULTIPLIER,
        }
    }
}

/// Per-bucket rolling-window counter for welcome-loan grants.
///
/// Thread-safe via enclosing `Arc<Mutex<>>` in the caller; not
/// internally locked since all mutation goes through `&mut self`
/// on the ledger hot path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WelcomeLoanLimiter {
    cfg: WelcomeLoanLimiterConfig,
    /// Per-bucket grant timestamps. Oldest-first; pruned on every
    /// query so memory stays bounded by `max_per_bucket_per_window *
    /// staked_multiplier` entries per bucket.
    grants: HashMap<String, Vec<u64>>,
}

impl WelcomeLoanLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(cfg: WelcomeLoanLimiterConfig) -> Self {
        Self {
            cfg,
            grants: HashMap::new(),
        }
    }

    pub fn config(&self) -> &WelcomeLoanLimiterConfig {
        &self.cfg
    }

    /// Number of buckets currently tracked.
    pub fn bucket_count(&self) -> usize {
        self.grants.len()
    }

    /// True if `bucket` is below its cap (scaled by `stake_proven`)
    /// over the current rolling window at `now_ms`.
    ///
    /// Side-effect free: safe to call repeatedly. Prunes its own
    /// bucket in the process (bounded, O(grants)).
    pub fn can_issue(&mut self, bucket: &str, stake_proven: bool, now_ms: u64) -> bool {
        self.prune_bucket(bucket, now_ms);
        let cap = self.effective_cap(stake_proven);
        self.grants
            .get(bucket)
            .map(|v| v.len() < cap)
            .unwrap_or(true)
    }

    /// Record a successful welcome-loan grant for `bucket` at `now_ms`.
    /// Caller should invoke this AFTER verifying `can_issue` returned
    /// `true` and the loan proposal was accepted.
    pub fn record(&mut self, bucket: &str, now_ms: u64) {
        self.prune_bucket(bucket, now_ms);
        self.grants
            .entry(bucket.to_string())
            .or_default()
            .push(now_ms);
    }

    /// Prune any bucket's entries older than the rolling window.
    /// An entry with timestamp `ts` is kept iff its age
    /// `now_ms - ts < window_ms`. This is an inclusive check at
    /// `ts == now_ms` (newly-recorded entries survive immediately)
    /// and an exclusive one at the window boundary (entries exactly
    /// `window_ms` old have rolled out).
    fn prune_bucket(&mut self, bucket: &str, now_ms: u64) {
        let window = self.cfg.window_ms;
        if let Some(v) = self.grants.get_mut(bucket) {
            v.retain(|ts| now_ms.saturating_sub(*ts) < window);
            if v.is_empty() {
                self.grants.remove(bucket);
            }
        }
    }

    /// Global GC — call periodically to drop buckets whose grants have
    /// all rolled out of the window, so the map doesn't grow unbounded.
    pub fn prune_stale(&mut self, now_ms: u64) -> usize {
        let window = self.cfg.window_ms;
        let mut removed = 0;
        self.grants.retain(|_, v| {
            v.retain(|ts| now_ms.saturating_sub(*ts) < window);
            let keep = !v.is_empty();
            if !keep {
                removed += 1;
            }
            keep
        });
        removed
    }

    fn effective_cap(&self, stake_proven: bool) -> usize {
        if stake_proven {
            self.cfg
                .max_per_bucket_per_window
                .saturating_mul(self.cfg.staked_multiplier)
        } else {
            self.cfg.max_per_bucket_per_window
        }
    }

    /// Current count of grants recorded for `bucket` within the
    /// active window. Useful for dashboards and test assertions.
    pub fn count_in_window(&mut self, bucket: &str, now_ms: u64) -> usize {
        self.prune_bucket(bucket, now_ms);
        self.grants.get(bucket).map(|v| v.len()).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Config ---

    #[test]
    fn default_config_is_24h_10_grants_10x_staked_bonus() {
        let c = WelcomeLoanLimiterConfig::default();
        assert_eq!(c.window_ms, 24 * 60 * 60 * 1_000);
        assert_eq!(c.max_per_bucket_per_window, 10);
        assert_eq!(c.staked_multiplier, 10);
    }

    // --- Basic gating ---

    #[test]
    fn empty_limiter_allows_all_first_grants() {
        let mut l = WelcomeLoanLimiter::new();
        assert!(l.can_issue("AWS-16509", false, 1_000));
        assert!(l.can_issue("GCP-15169", false, 1_000));
    }

    #[test]
    fn cap_enforced_within_window() {
        let mut l = WelcomeLoanLimiter::new();
        let bucket = "AWS-16509";
        // Grant 10 loans — all allowed, then the 11th is blocked.
        for i in 0..10 {
            assert!(l.can_issue(bucket, false, 1_000 + i as u64));
            l.record(bucket, 1_000 + i as u64);
        }
        assert!(!l.can_issue(bucket, false, 2_000));
    }

    #[test]
    fn cap_is_per_bucket_not_global() {
        // Different bucket keys must have independent counters.
        let mut l = WelcomeLoanLimiter::new();
        for _ in 0..10 {
            l.record("bucket-a", 1_000);
        }
        assert!(!l.can_issue("bucket-a", false, 1_000));
        // "bucket-b" must still be allowed.
        assert!(l.can_issue("bucket-b", false, 1_000));
    }

    #[test]
    fn window_rolls_grants_out() {
        let mut l = WelcomeLoanLimiter::new();
        let bucket = "AWS-16509";
        for _ in 0..10 {
            l.record(bucket, 0); // all at ts=0
        }
        assert!(!l.can_issue(bucket, false, 1_000));
        // 24h + epsilon later, all old grants age out.
        let far_future = 24 * 60 * 60 * 1_000 + 1;
        assert!(l.can_issue(bucket, false, far_future));
    }

    // --- Stake-proven multiplier ---

    #[test]
    fn stake_proven_grants_get_10x_cap() {
        let mut l = WelcomeLoanLimiter::new();
        let bucket = "AWS-16509";
        // Fill 10 unstaked grants.
        for _ in 0..10 {
            l.record(bucket, 100);
        }
        // Unstaked → blocked.
        assert!(!l.can_issue(bucket, false, 101));
        // Stake-proven → still allowed up to 100 total.
        assert!(l.can_issue(bucket, true, 101));
    }

    #[test]
    fn stake_proven_cap_is_still_finite() {
        let mut l = WelcomeLoanLimiter::new();
        let bucket = "AWS-16509";
        for i in 0..100 {
            l.record(bucket, 100 + i);
        }
        // 100 staked grants = exactly the cap (10 × 10).
        assert!(!l.can_issue(bucket, true, 200));
    }

    // --- Bookkeeping / pruning ---

    #[test]
    fn count_in_window_reflects_recorded_grants() {
        let mut l = WelcomeLoanLimiter::new();
        let bucket = "x";
        for _ in 0..5 {
            l.record(bucket, 100);
        }
        assert_eq!(l.count_in_window(bucket, 200), 5);
    }

    #[test]
    fn count_in_window_prunes_stale_entries() {
        let mut l = WelcomeLoanLimiter::new();
        let bucket = "x";
        l.record(bucket, 100);
        l.record(bucket, 200);
        // Very-far-future query: all entries are stale.
        let far = DEFAULT_WELCOME_WINDOW_MS + 1_000_000;
        assert_eq!(l.count_in_window(bucket, far), 0);
        // Bucket removed when empty.
        assert_eq!(l.bucket_count(), 0);
    }

    #[test]
    fn prune_stale_removes_completely_aged_buckets() {
        let mut l = WelcomeLoanLimiter::new();
        l.record("old", 0);
        l.record("fresh", 10_000);
        let now = 10_000 + 1_000; // still well within window for both
        let removed = l.prune_stale(now);
        assert_eq!(removed, 0); // nothing aged out yet

        let far_future = DEFAULT_WELCOME_WINDOW_MS + 10_000 + 1;
        let removed = l.prune_stale(far_future);
        // Both entries (from 0 and 10_000) pre-date the window cutoff
        // of DEFAULT_WELCOME_WINDOW_MS ago, so both buckets should drop.
        assert_eq!(removed, 2);
        assert_eq!(l.bucket_count(), 0);
    }

    #[test]
    fn re_record_after_prune_works_fresh() {
        let mut l = WelcomeLoanLimiter::new();
        let bucket = "AWS-16509";
        for _ in 0..10 {
            l.record(bucket, 100);
        }
        let far = DEFAULT_WELCOME_WINDOW_MS + 200;
        // After window roll, 10 new grants should all pass.
        for i in 0..10 {
            assert!(l.can_issue(bucket, false, far + i));
            l.record(bucket, far + i);
        }
        assert!(!l.can_issue(bucket, false, far + 10));
    }

    // --- Config custom ---

    #[test]
    fn custom_config_honors_tighter_cap() {
        let mut l = WelcomeLoanLimiter::with_config(WelcomeLoanLimiterConfig {
            window_ms: 1_000,
            max_per_bucket_per_window: 2,
            staked_multiplier: 1,
        });
        assert!(l.can_issue("x", false, 0));
        l.record("x", 0);
        assert!(l.can_issue("x", false, 100));
        l.record("x", 100);
        // Two grants, cap hit.
        assert!(!l.can_issue("x", false, 200));
    }

    #[test]
    fn config_serde_roundtrips_json() {
        let c = WelcomeLoanLimiterConfig::default();
        let s = serde_json::to_string(&c).unwrap();
        let back: WelcomeLoanLimiterConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back.window_ms, c.window_ms);
        assert_eq!(back.max_per_bucket_per_window, c.max_per_bucket_per_window);
        assert_eq!(back.staked_multiplier, c.staked_multiplier);
    }
}
