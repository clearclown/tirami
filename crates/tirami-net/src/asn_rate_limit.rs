//! Phase 17 Wave 2.3 — per-ASN (Autonomous System Number) rate limiting.
//!
//! # Why ASN and not just per-peer?
//!
//! The existing 500 msg/s per-peer bucket in `transport::read_peer_messages`
//! defends against one chatty node, but not against a cloud-based Sybil.
//! An attacker can spin up 100 IPs inside the AWS ASN (7.1 B IP → ASN 16509)
//! and receive 100 × 500 = 50 000 msg/s worth of slots — more than any real
//! operator network ever produces. By quota-ing *at the ASN level*, we
//! collapse that advantage: all AWS traffic shares the same 5 000 msg/s
//! bucket regardless of how many distinct IPs participate.
//!
//! # Design
//!
//! * [`AsnResolver`] trait — pluggable "given an IP, what ASN?"
//!   * [`StaticAsnResolver`] uses an in-memory map, ships in this module
//!     and is the test fixture.
//!   * A future `MaxMindAsnResolver` wraps the MaxMind GeoLite2-ASN DB.
//! * [`AsnRateLimiter`] owns one [`TokenBucket`] per observed ASN,
//!   lazily created on first hit. Capacity + refill rate are
//!   configurable. Unknown ASNs (resolver returns `None`) are treated
//!   as a single shared bucket — a pragmatic default so peers outside
//!   the MaxMind DB coverage still see a ceiling.
//!
//! # Integration hook
//!
//! The transport can call [`AsnRateLimiter::take(ip)`] before delivering
//! each message; `false` = drop. This wave ships the type + tests; a
//! follow-up wires it into `read_peer_messages` behind the new
//! `config.asn_rate_limit_enabled` flag, so operators who haven't
//! downloaded the MaxMind DB don't pay any accuracy cost.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// Autonomous System Number. RFC-5396 says this is 32 bits (new 4-byte
/// ASNs included); we store it as `u32` and use `0` as a sentinel for
/// "unknown / unresolved".
pub type Asn = u32;

/// The sentinel ASN used when the resolver returns `None`. Messages
/// from unresolved IPs share this single bucket, which keeps the
/// overall system bounded even without an IP→ASN database loaded.
pub const UNKNOWN_ASN: Asn = 0;

// ---------------------------------------------------------------------------
// AsnResolver trait
// ---------------------------------------------------------------------------

/// Trait for resolving an IP address to an ASN. Must be fast (called
/// on every incoming message in the rate-limit hot path) and thread-safe.
pub trait AsnResolver: Send + Sync {
    /// Resolve `ip` to its owning ASN. Return `None` if the resolver
    /// has no answer (e.g. the IP isn't in its database); callers
    /// translate that to [`UNKNOWN_ASN`].
    fn resolve(&self, ip: IpAddr) -> Option<Asn>;
}

/// Test / no-database resolver backed by an in-memory `HashMap`.
/// Operators running without the MaxMind DB can still use this to
/// pin known-hostile /24 subnets to a shared ASN.
#[derive(Debug, Clone, Default)]
pub struct StaticAsnResolver {
    map: HashMap<IpAddr, Asn>,
}

impl StaticAsnResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Associate `ip` with `asn`. Subsequent resolves return `Some(asn)`.
    pub fn insert(&mut self, ip: IpAddr, asn: Asn) {
        self.map.insert(ip, asn);
    }

    /// Bulk load from `(IP, ASN)` pairs.
    pub fn with_entries<I: IntoIterator<Item = (IpAddr, Asn)>>(entries: I) -> Self {
        let mut r = Self::new();
        for (ip, asn) in entries {
            r.insert(ip, asn);
        }
        r
    }
}

impl AsnResolver for StaticAsnResolver {
    fn resolve(&self, ip: IpAddr) -> Option<Asn> {
        self.map.get(&ip).copied()
    }
}

// ---------------------------------------------------------------------------
// Token bucket (per-ASN)
// ---------------------------------------------------------------------------

/// Simple token bucket. Constant refill rate, capacity-bounded.
///
/// Not exported outside this module — the caller interacts via
/// [`AsnRateLimiter::take`].
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Current fractional token count. f64 so 1 s of 5 000 msg/s
    /// refill doesn't need to add up through u64 integer rounding.
    tokens: f64,
    /// Last instant we refilled; refills happen on every `take`.
    last_refill: Instant,
}

impl TokenBucket {
    fn new(initial: f64) -> Self {
        Self {
            tokens: initial,
            last_refill: Instant::now(),
        }
    }

    /// Refill based on elapsed time, then attempt to consume one token.
    /// Returns `true` on success, `false` if the bucket was empty.
    fn take(&mut self, rate_per_sec: f64, capacity: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * rate_per_sec).min(capacity);
        self.last_refill = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

// ---------------------------------------------------------------------------
// AsnRateLimiter
// ---------------------------------------------------------------------------

/// Configuration for [`AsnRateLimiter`].
#[derive(Debug, Clone, Copy)]
pub struct AsnRateLimitConfig {
    /// Sustained message rate per ASN per second.
    /// Default matches the plan: 5 000 msg/s — enough for an entire
    /// cloud region, small enough to make a single-ASN Sybil pointless.
    pub rate_per_sec: f64,
    /// Burst size — maximum tokens the bucket can hold at once.
    /// Default 2× rate so a momentary rush of handshakes doesn't drop.
    pub burst: f64,
    /// How long a per-ASN bucket can idle before being garbage-collected
    /// from the map. Prevents unbounded growth when scans touch many ASNs.
    pub idle_ttl: Duration,
}

impl Default for AsnRateLimitConfig {
    fn default() -> Self {
        Self {
            rate_per_sec: 5_000.0,
            burst: 10_000.0,
            idle_ttl: Duration::from_secs(5 * 60),
        }
    }
}

/// Per-ASN rate limiter. Instantiate once per transport; call
/// [`Self::take`] on every inbound message.
pub struct AsnRateLimiter {
    resolver: Box<dyn AsnResolver>,
    cfg: AsnRateLimitConfig,
    buckets: HashMap<Asn, TokenBucket>,
    /// Last `take` timestamp per ASN; used for idle GC so a one-off
    /// scan doesn't permanently pin a bucket.
    last_seen: HashMap<Asn, Instant>,
}

impl std::fmt::Debug for AsnRateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsnRateLimiter")
            .field("cfg", &self.cfg)
            .field("buckets", &self.buckets.len())
            .finish()
    }
}

impl AsnRateLimiter {
    /// Build a limiter with the given resolver and the default config.
    pub fn new<R: AsnResolver + 'static>(resolver: R) -> Self {
        Self::with_config(resolver, AsnRateLimitConfig::default())
    }

    /// Build with an explicit config.
    pub fn with_config<R: AsnResolver + 'static>(resolver: R, cfg: AsnRateLimitConfig) -> Self {
        Self {
            resolver: Box::new(resolver),
            cfg,
            buckets: HashMap::new(),
            last_seen: HashMap::new(),
        }
    }

    /// Attempt to consume a token for `ip`'s ASN. Returns `true` if the
    /// message should be processed, `false` if the ASN has exceeded its
    /// quota (caller drops the message).
    pub fn take(&mut self, ip: IpAddr) -> bool {
        let asn = self.resolver.resolve(ip).unwrap_or(UNKNOWN_ASN);
        let bucket = self
            .buckets
            .entry(asn)
            .or_insert_with(|| TokenBucket::new(self.cfg.burst));
        let ok = bucket.take(self.cfg.rate_per_sec, self.cfg.burst);
        self.last_seen.insert(asn, Instant::now());
        ok
    }

    /// Drop buckets that have been idle longer than `cfg.idle_ttl`.
    /// Intended to run on a slow timer (e.g. every 60 s).
    pub fn prune_idle(&mut self) -> usize {
        let now = Instant::now();
        let ttl = self.cfg.idle_ttl;
        let stale: Vec<Asn> = self
            .last_seen
            .iter()
            .filter(|(_, t)| now.duration_since(**t) > ttl)
            .map(|(a, _)| *a)
            .collect();
        for asn in &stale {
            self.buckets.remove(asn);
            self.last_seen.remove(asn);
        }
        stale.len()
    }

    /// Total number of distinct ASNs currently tracked.
    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    fn static_resolver(entries: &[(u8, u8, u8, u8, Asn)]) -> StaticAsnResolver {
        StaticAsnResolver::with_entries(
            entries.iter().map(|&(a, b, c, d, asn)| (ip(a, b, c, d), asn)),
        )
    }

    #[test]
    fn resolver_returns_none_for_unknown_ip() {
        let r = StaticAsnResolver::new();
        assert!(r.resolve(ip(1, 2, 3, 4)).is_none());
    }

    #[test]
    fn resolver_returns_configured_asn() {
        let r = static_resolver(&[(10, 0, 0, 1, 16509)]);
        assert_eq!(r.resolve(ip(10, 0, 0, 1)), Some(16509));
        assert_eq!(r.resolve(ip(10, 0, 0, 2)), None);
    }

    #[test]
    fn first_take_on_new_asn_always_succeeds() {
        // A fresh bucket starts at `burst`; one take must always pass.
        let mut l = AsnRateLimiter::new(static_resolver(&[(1, 1, 1, 1, 42)]));
        assert!(l.take(ip(1, 1, 1, 1)));
    }

    #[test]
    fn many_takes_above_rate_are_rejected() {
        // Very small burst + high-rate confirms drop behavior.
        let mut l = AsnRateLimiter::with_config(
            static_resolver(&[(1, 1, 1, 1, 42)]),
            AsnRateLimitConfig {
                rate_per_sec: 10.0,
                burst: 3.0,
                idle_ttl: Duration::from_secs(60),
            },
        );
        // Burst of 3 allowed, 4th must fail (no time for refill).
        assert!(l.take(ip(1, 1, 1, 1)));
        assert!(l.take(ip(1, 1, 1, 1)));
        assert!(l.take(ip(1, 1, 1, 1)));
        assert!(!l.take(ip(1, 1, 1, 1)));
    }

    #[test]
    fn different_asns_have_independent_buckets() {
        // Exhausting ASN A's bucket must not affect ASN B.
        let mut l = AsnRateLimiter::with_config(
            static_resolver(&[(1, 0, 0, 1, 100), (2, 0, 0, 1, 200)]),
            AsnRateLimitConfig {
                rate_per_sec: 1.0,
                burst: 1.0,
                idle_ttl: Duration::from_secs(60),
            },
        );
        assert!(l.take(ip(1, 0, 0, 1))); // ASN 100 burst
        assert!(!l.take(ip(1, 0, 0, 1))); // ASN 100 drained
        assert!(l.take(ip(2, 0, 0, 1))); // ASN 200 still has tokens
    }

    #[test]
    fn many_ips_same_asn_share_one_bucket() {
        // This is the core Sybil defense: 50 IPs, all same ASN, still
        // share one token bucket. Previously each would have gotten
        // its own per-peer bucket.
        let mut resolver = StaticAsnResolver::new();
        for i in 0..50 {
            resolver.insert(ip(10, 0, 0, i), 16509); // all AWS
        }
        let mut l = AsnRateLimiter::with_config(
            resolver,
            AsnRateLimitConfig {
                rate_per_sec: 1.0,
                burst: 3.0,
                idle_ttl: Duration::from_secs(60),
            },
        );
        let mut accepted = 0;
        for i in 0..50 {
            if l.take(ip(10, 0, 0, i)) {
                accepted += 1;
            }
        }
        // Exactly `burst` (3) should be accepted — the rest share the
        // drained bucket.
        assert_eq!(accepted, 3);
    }

    #[test]
    fn unresolved_ip_falls_back_to_unknown_asn_bucket() {
        // An IP not in the resolver's map must still be rate-limited
        // (vs being silently waved through).
        let mut l = AsnRateLimiter::with_config(
            StaticAsnResolver::new(),
            AsnRateLimitConfig {
                rate_per_sec: 1.0,
                burst: 1.0,
                idle_ttl: Duration::from_secs(60),
            },
        );
        assert!(l.take(ip(9, 9, 9, 1))); // burst allowance
        // Any subsequent unresolved IP shares the UNKNOWN_ASN bucket.
        assert!(!l.take(ip(9, 9, 9, 2)));
    }

    #[test]
    fn unknown_asn_sentinel_is_reachable() {
        let mut l = AsnRateLimiter::new(StaticAsnResolver::new());
        l.take(ip(9, 9, 9, 9));
        assert!(l.buckets.contains_key(&UNKNOWN_ASN));
    }

    #[test]
    fn prune_idle_drops_stale_buckets() {
        let cfg = AsnRateLimitConfig {
            rate_per_sec: 1.0,
            burst: 3.0,
            // Intentionally zero so every bucket is "stale" immediately
            // on the next prune call.
            idle_ttl: Duration::from_millis(0),
        };
        let mut l = AsnRateLimiter::with_config(
            static_resolver(&[(1, 0, 0, 1, 100)]),
            cfg,
        );
        l.take(ip(1, 0, 0, 1));
        std::thread::sleep(Duration::from_millis(2));
        let dropped = l.prune_idle();
        assert_eq!(dropped, 1);
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn len_and_is_empty_reflect_bucket_count() {
        let mut l = AsnRateLimiter::new(static_resolver(&[
            (1, 0, 0, 1, 100),
            (2, 0, 0, 1, 200),
        ]));
        assert!(l.is_empty());
        l.take(ip(1, 0, 0, 1));
        assert_eq!(l.len(), 1);
        l.take(ip(2, 0, 0, 1));
        assert_eq!(l.len(), 2);
    }
}
