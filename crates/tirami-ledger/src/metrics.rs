//! Prometheus / OpenMetrics export for Tirami telemetry.
//!
//! Exposes:
//! - tirami_cu_contributed_total   (gauge, per-node_id)
//! - tirami_cu_consumed_total      (gauge, per-node_id)
//! - tirami_reputation             (gauge, per-node_id)
//! - tirami_trade_count_total      (counter, global)
//! - tirami_active_loan_count      (gauge, global)
//! - tirami_pool_total_trm         (gauge, global)
//! - tirami_pool_reserve_ratio     (gauge, global)
//! - tirami_collusion_tight_cluster_score  (gauge, per-node_id)
//! - tirami_collusion_volume_spike_score   (gauge, per-node_id)
//! - tirami_collusion_round_robin_score    (gauge, per-node_id)
//! - tirami_collusion_trust_penalty        (gauge, per-node_id)

use crate::collusion::CollusionDetector;
use crate::ledger::ComputeLedger;
use prometheus::{
    Encoder, Gauge, GaugeVec, IntCounter, IntGauge, Opts, Registry, TextEncoder,
};

/// Holds all Prometheus metrics for a Forge node.
/// Placeholder NodeId used by HTTP `/v1/chat/completions` when the
/// caller does not supply an `X-Tirami-Node-Id` header. Every byte
/// is 0xFF so the sentinel is easy to spot in ledger dumps. The
/// metrics layer filters this id out so it never appears as a
/// node_id label on Prometheus dashboards (fix #83).
const ANONYMOUS_CONSUMER_SENTINEL: [u8; 32] = [0xFFu8; 32];

pub(crate) fn is_anonymous_consumer(node_id: &tirami_core::NodeId) -> bool {
    node_id.0 == ANONYMOUS_CONSUMER_SENTINEL
}

/// Fix #85 — round a Prometheus gauge value to 9 decimal places so
/// long-tail f64 drift (e.g. `0.9999999993809524` from `1.0 -
/// minted/TOTAL_SUPPLY` with tens minted against 21 B) doesn't
/// surface on operator dashboards.
fn round_to_9dp(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    (x * 1_000_000_000.0).round() / 1_000_000_000.0
}

pub struct TiramiMetrics {
    pub registry: Registry,
    pub cu_contributed: GaugeVec,
    pub cu_consumed: GaugeVec,
    pub reputation: GaugeVec,
    pub trade_count: IntCounter,
    pub active_loan_count: IntGauge,
    pub pool_total_trm: IntGauge,
    pub pool_reserve_ratio: Gauge,
    pub collusion_tight: GaugeVec,
    pub collusion_spike: GaugeVec,
    pub collusion_robin: GaugeVec,
    pub collusion_penalty: GaugeVec,
    // Phase 13 governance metrics
    pub active_proposals: IntGauge,
    pub total_votes: IntGauge,
    // Phase 13 tokenomics metrics
    pub total_minted: IntGauge,
    pub supply_factor: Gauge,
    pub current_epoch: IntGauge,
    pub yield_rate: Gauge,
    pub total_staked: IntGauge,
    pub total_burned: IntGauge,
    pub referral_bonus_minted: IntGauge,
}

impl TiramiMetrics {
    /// Create a new TiramiMetrics with its own isolated Registry.
    pub fn new() -> Self {
        let registry = Registry::new();

        let cu_contributed = GaugeVec::new(
            Opts::new(
                "tirami_cu_contributed_total",
                "Total CU contributed (earned) by a node",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let cu_consumed = GaugeVec::new(
            Opts::new(
                "tirami_cu_consumed_total",
                "Total CU consumed (spent) by a node",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let reputation = GaugeVec::new(
            Opts::new("tirami_reputation", "Reputation score for a node (0.0–1.0)"),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let trade_count = IntCounter::with_opts(Opts::new(
            "tirami_trade_count_total",
            "Total number of trades recorded in the ledger",
        ))
        .expect("valid counter opts");

        let active_loan_count = IntGauge::with_opts(Opts::new(
            "tirami_active_loan_count",
            "Number of currently active loans in the lending pool",
        ))
        .expect("valid gauge opts");

        let pool_total_trm = IntGauge::with_opts(Opts::new(
            "tirami_pool_total_trm",
            "Total CU deposited into the lending pool",
        ))
        .expect("valid gauge opts");

        let pool_reserve_ratio = Gauge::with_opts(Opts::new(
            "tirami_pool_reserve_ratio",
            "Lending pool reserve ratio (available / total)",
        ))
        .expect("valid gauge opts");

        let collusion_tight = GaugeVec::new(
            Opts::new(
                "tirami_collusion_tight_cluster_score",
                "Tight-cluster collusion sub-score for a node (0.0–1.0)",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let collusion_spike = GaugeVec::new(
            Opts::new(
                "tirami_collusion_volume_spike_score",
                "Volume-spike collusion sub-score for a node (0.0–1.0)",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let collusion_robin = GaugeVec::new(
            Opts::new(
                "tirami_collusion_round_robin_score",
                "Round-robin collusion sub-score for a node (0.0–1.0)",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let collusion_penalty = GaugeVec::new(
            Opts::new(
                "tirami_collusion_trust_penalty",
                "Final trust penalty applied to node reputation (0.0–0.5)",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        // Phase 13 governance gauges
        let active_proposals = IntGauge::with_opts(Opts::new(
            "tirami_active_proposals",
            "Number of currently active governance proposals",
        ))
        .expect("valid gauge opts");

        let total_votes = IntGauge::with_opts(Opts::new(
            "tirami_total_votes",
            "Total votes cast across all governance proposals",
        ))
        .expect("valid gauge opts");

        // Phase 13 tokenomics gauges
        let total_minted = IntGauge::with_opts(Opts::new(
            "tirami_total_minted",
            "Total TRM minted so far (counts toward 21B cap)",
        ))
        .expect("valid gauge opts");

        let supply_factor = Gauge::with_opts(Opts::new(
            "tirami_supply_factor",
            "Supply factor: fraction of TRM cap remaining (1.0 at genesis, 0.0 at cap)",
        ))
        .expect("valid gauge opts");

        let current_epoch = IntGauge::with_opts(Opts::new(
            "tirami_current_epoch",
            "Current halving epoch (0 = genesis, increases as supply is consumed)",
        ))
        .expect("valid gauge opts");

        let yield_rate = Gauge::with_opts(Opts::new(
            "tirami_yield_rate",
            "Current availability yield rate per hour (halves each epoch)",
        ))
        .expect("valid gauge opts");

        let total_staked = IntGauge::with_opts(Opts::new(
            "tirami_total_staked",
            "Total TRM currently locked in staking contracts",
        ))
        .expect("valid gauge opts");

        let total_burned = IntGauge::with_opts(Opts::new(
            "tirami_total_burned",
            "Total TRM burned via slashing (permanently removed from circulation)",
        ))
        .expect("valid gauge opts");

        let referral_bonus_minted = IntGauge::with_opts(Opts::new(
            "tirami_referral_bonus_minted",
            "Total TRM minted as referral bonuses",
        ))
        .expect("valid gauge opts");

        // Register all metrics with the isolated registry.
        registry
            .register(Box::new(cu_contributed.clone()))
            .expect("register cu_contributed");
        registry
            .register(Box::new(cu_consumed.clone()))
            .expect("register cu_consumed");
        registry
            .register(Box::new(reputation.clone()))
            .expect("register reputation");
        registry
            .register(Box::new(trade_count.clone()))
            .expect("register trade_count");
        registry
            .register(Box::new(active_loan_count.clone()))
            .expect("register active_loan_count");
        registry
            .register(Box::new(pool_total_trm.clone()))
            .expect("register pool_total_trm");
        registry
            .register(Box::new(pool_reserve_ratio.clone()))
            .expect("register pool_reserve_ratio");
        registry
            .register(Box::new(collusion_tight.clone()))
            .expect("register collusion_tight");
        registry
            .register(Box::new(collusion_spike.clone()))
            .expect("register collusion_spike");
        registry
            .register(Box::new(collusion_robin.clone()))
            .expect("register collusion_robin");
        registry
            .register(Box::new(collusion_penalty.clone()))
            .expect("register collusion_penalty");
        registry
            .register(Box::new(active_proposals.clone()))
            .expect("register active_proposals");
        registry
            .register(Box::new(total_votes.clone()))
            .expect("register total_votes");
        registry
            .register(Box::new(total_minted.clone()))
            .expect("register total_minted");
        registry
            .register(Box::new(supply_factor.clone()))
            .expect("register supply_factor");
        registry
            .register(Box::new(current_epoch.clone()))
            .expect("register current_epoch");
        registry
            .register(Box::new(yield_rate.clone()))
            .expect("register yield_rate");
        registry
            .register(Box::new(total_staked.clone()))
            .expect("register total_staked");
        registry
            .register(Box::new(total_burned.clone()))
            .expect("register total_burned");
        registry
            .register(Box::new(referral_bonus_minted.clone()))
            .expect("register referral_bonus_minted");

        Self {
            registry,
            cu_contributed,
            cu_consumed,
            reputation,
            trade_count,
            active_loan_count,
            pool_total_trm,
            pool_reserve_ratio,
            collusion_tight,
            collusion_spike,
            collusion_robin,
            collusion_penalty,
            active_proposals,
            total_votes,
            total_minted,
            supply_factor,
            current_epoch,
            yield_rate,
            total_staked,
            total_burned,
            referral_bonus_minted,
        }
    }

    /// Snapshot ledger state into all metrics.
    ///
    /// This is idempotent and cheap to call — Prometheus gauges are set (not
    /// incremented), so calling `observe` twice in a row is safe.
    ///
    /// `staking_pool` and `referral_tracker` are optional — if None the Phase 13
    /// tokenomics gauges remain at their last set value (zero at startup).
    pub fn observe(
        &self,
        ledger: &ComputeLedger,
        now_ms: u64,
    ) {
        self.observe_with_tokenomics(ledger, now_ms, None, None);
    }

    /// Extended observe that also snapshots Phase 13 tokenomics state.
    pub fn observe_with_tokenomics(
        &self,
        ledger: &ComputeLedger,
        now_ms: u64,
        staking_pool: Option<&crate::staking::StakingPool>,
        referral_tracker: Option<&crate::referral::ReferralTracker>,
    ) {
        self.observe_full(ledger, now_ms, staking_pool, referral_tracker, None);
    }

    /// Full observe including governance state.
    pub fn observe_full(
        &self,
        ledger: &ComputeLedger,
        now_ms: u64,
        staking_pool: Option<&crate::staking::StakingPool>,
        referral_tracker: Option<&crate::referral::ReferralTracker>,
        governance: Option<&crate::governance::GovernanceState>,
    ) {
        // Per-node balance and reputation metrics.
        for balance in ledger.balances.values() {
            // Fix #83 — skip the anonymous-consumer sentinel. HTTP
            // `record_api_trade` uses NodeId([255u8; 32]) as a
            // placeholder consumer when the caller omits the
            // `X-Tirami-Node-Id` header. That's an accounting
            // bucket, not a real peer, so it shouldn't show up as
            // a node_id label on Prometheus dashboards.
            if is_anonymous_consumer(&balance.node_id) {
                continue;
            }
            let hex = balance.node_id.to_hex();
            let label = [hex.as_str()];

            self.cu_contributed
                .with_label_values(&label)
                .set(balance.contributed as f64);

            self.cu_consumed
                .with_label_values(&label)
                .set(balance.consumed as f64);

            self.reputation
                .with_label_values(&label)
                .set(balance.reputation);
        }

        // Global trade count.
        // IntCounter only supports increment — reset by tracking the delta.
        let current_count = ledger.trade_log.len() as u64;
        let reported = self.trade_count.get() as u64;
        if current_count > reported {
            self.trade_count.inc_by(current_count - reported);
        }

        // Lending pool gauges.
        let pool = ledger.lending_pool_status();
        self.active_loan_count
            .set(pool.active_loan_count as i64);
        self.pool_total_trm.set(pool.total_pool_cu as i64);
        self.pool_reserve_ratio.set(pool.reserve_ratio);

        // Collusion scores — analyze every node that appears in the trade log
        // within the analysis window.
        let trades = &ledger.trade_log;
        if !trades.is_empty() {
            // Collect unique node IDs from the full trade log.
            let mut nodes: std::collections::HashSet<tirami_core::NodeId> =
                std::collections::HashSet::new();
            let window_start = now_ms
                .saturating_sub(crate::collusion::COLLUSION_WINDOW_MS);
            for t in trades.iter().filter(|t| {
                t.timestamp >= window_start && t.timestamp <= now_ms
            }) {
                nodes.insert(t.provider.clone());
                nodes.insert(t.consumer.clone());
            }

            for node in &nodes {
                if is_anonymous_consumer(node) {
                    continue;
                }
                let report =
                    CollusionDetector::analyze_node(trades, node, now_ms);
                let hex = node.to_hex();
                let label = [hex.as_str()];
                self.collusion_tight
                    .with_label_values(&label)
                    .set(report.tight_cluster_score);
                self.collusion_spike
                    .with_label_values(&label)
                    .set(report.volume_spike_score);
                self.collusion_robin
                    .with_label_values(&label)
                    .set(report.round_robin_score);
                self.collusion_penalty
                    .with_label_values(&label)
                    .set(report.trust_penalty);
            }
        }

        // Phase 13 tokenomics gauges — ledger-derived.
        // Fix #85 — round supply_factor / yield_rate to 9 decimals
        // (nano-unit precision, well below any operationally
        // meaningful alert threshold) to silence f64 long-tail
        // noise on dashboards.
        let minted = ledger.total_minted;
        self.total_minted.set(minted as i64);
        self.supply_factor
            .set(round_to_9dp(crate::tokenomics::supply_factor(minted)));
        self.current_epoch
            .set(crate::tokenomics::current_epoch(minted) as i64);
        self.yield_rate
            .set(round_to_9dp(crate::tokenomics::epoch_yield_rate(minted)));

        // Optional staking pool data.
        if let Some(pool) = staking_pool {
            self.total_staked.set(pool.total_staked() as i64);
            self.total_burned.set(pool.total_burned as i64);
        }

        // Optional referral tracker data.
        if let Some(tracker) = referral_tracker {
            self.referral_bonus_minted
                .set(tracker.total_bonus_minted as i64);
        }

        // Optional governance state.
        if let Some(gov) = governance {
            self.active_proposals
                .set(gov.active_proposals().len() as i64);
            let total_v: usize = gov.votes.values().map(|v| v.len()).sum();
            self.total_votes.set(total_v as i64);
        }
    }

    /// Encode the registry as OpenMetrics text body for the HTTP /metrics
    /// endpoint.
    pub fn encode(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buf = Vec::new();
        encoder.encode(&metric_families, &mut buf)?;
        Ok(String::from_utf8(buf).unwrap_or_default())
    }
}

impl Default for TiramiMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::TradeRecord;
    use tirami_core::NodeId;

    fn make_trade(
        provider: [u8; 32],
        consumer: [u8; 32],
        cu: u64,
        ts: u64,
    ) -> TradeRecord {
        TradeRecord {
            provider: NodeId(provider),
            consumer: NodeId(consumer),
            trm_amount: cu,
            tokens_processed: cu / 10,
            timestamp: ts,
            model_id: "test".into(),
            flops_estimated: 0,
                    nonce: [0u8; 16],
        }
    }

    #[test]
    fn test_new_metrics_registers_all_gauges() {
        // Populate the ledger with one node so that GaugeVec metrics emit samples.
        // prometheus omits GaugeVec HELP lines when no label values have been set.
        let mut ledger = ComputeLedger::new();
        let node = NodeId([1u8; 32]);
        ledger.record_consumption(&node, 10);
        let metrics = TiramiMetrics::new();
        metrics.observe(&ledger, 1_700_000_000_000);
        let output = metrics.encode().unwrap();
        assert!(
            output.contains("tirami_cu_contributed_total"),
            "missing tirami_cu_contributed_total in:\n{output}"
        );
        assert!(
            output.contains("tirami_reputation"),
            "missing tirami_reputation"
        );
        // collusion gauges only appear if trades exist; check a global metric instead.
        assert!(
            output.contains("tirami_trade_count_total"),
            "missing tirami_trade_count_total"
        );
        // Regression: ensure the legacy forge_* prefix is no longer emitted
        // (fix #76).
        assert!(
            !output.contains("forge_cu_contributed_total"),
            "legacy forge_cu_contributed_total should be gone"
        );
        assert!(
            !output.contains("forge_reputation"),
            "legacy forge_reputation should be gone"
        );
        // Verify the format includes HELP/TYPE metadata.
        assert!(output.contains("# HELP"), "missing # HELP");
        assert!(output.contains("# TYPE"), "missing # TYPE");
    }

    #[test]
    fn test_observe_populates_reputation_gauge() {
        let mut ledger = ComputeLedger::new();
        let node = NodeId([42u8; 32]);
        ledger.record_consumption(&node, 100);
        let metrics = TiramiMetrics::new();
        metrics.observe(&ledger, 1_700_000_000_000);
        let output = metrics.encode().unwrap();
        assert!(output.contains("tirami_reputation"), "tirami_reputation missing");
        assert!(
            output.contains(&node.to_hex()),
            "node hex {} not in output",
            node.to_hex()
        );
    }

    // ------------------------------------------------------------------
    // Fix #83 — anonymous consumer sentinel must not leak to metrics
    // ------------------------------------------------------------------

    #[test]
    fn is_anonymous_consumer_recognises_sentinel() {
        let sentinel = NodeId([0xFFu8; 32]);
        assert!(super::is_anonymous_consumer(&sentinel));
        let real = NodeId([0x42u8; 32]);
        assert!(!super::is_anonymous_consumer(&real));
    }

    #[test]
    fn test_observe_skips_anonymous_consumer_sentinel() {
        // Simulate the state after a self-served HTTP chat: the
        // ledger has a real node AND the anonymous sentinel in
        // `balances`. Metrics must only expose the real node.
        let mut ledger = ComputeLedger::new();
        let real = NodeId([0x42u8; 32]);
        let anon = NodeId([0xFFu8; 32]);
        ledger.execute_trade(&TradeRecord {
            provider: real.clone(),
            consumer: anon.clone(),
            trm_amount: 5,
            tokens_processed: 5,
            timestamp: 1_700_000_000_000,
            model_id: "test".to_string(),
            flops_estimated: 0,
            nonce: [0u8; 16],
        });
        let metrics = TiramiMetrics::new();
        metrics.observe(&ledger, 1_700_000_000_000);
        let output = metrics.encode().unwrap();
        assert!(
            output.contains(&real.to_hex()),
            "real node must appear in /metrics"
        );
        assert!(
            !output.contains(&"ff".repeat(32)),
            "anonymous sentinel must NOT appear in /metrics:\n{output}"
        );
    }

    #[test]
    fn test_observe_populates_collusion_metrics_for_traders() {
        // Build a ledger with >= MIN_TRADES_FOR_ANALYSIS (10) trades so that
        // the collusion analyzer produces non-zero scores.
        let mut ledger = ComputeLedger::new();
        let a = NodeId([10u8; 32]);
        let b = NodeId([11u8; 32]);

        // Use a fixed "now" so timestamps are in the window.
        let now_ms: u64 = 1_700_000_000_000;
        let window_start = now_ms - crate::collusion::COLLUSION_WINDOW_MS;

        for i in 0..18u64 {
            // All trades between a and b — high tight-cluster concentration.
            ledger.trade_log.push(make_trade(
                [10u8; 32],
                [11u8; 32],
                100,
                window_start + i * 60_000,
            ));
        }

        let metrics = TiramiMetrics::new();
        metrics.observe(&ledger, now_ms);
        let output = metrics.encode().unwrap();

        assert!(
            output.contains("tirami_collusion_tight_cluster_score"),
            "tight_cluster metric missing"
        );
        assert!(
            output.contains("tirami_collusion_trust_penalty"),
            "trust_penalty metric missing"
        );
        // Both nodes should appear.
        assert!(
            output.contains(&a.to_hex()),
            "node a hex missing from output"
        );
        assert!(
            output.contains(&b.to_hex()),
            "node b hex missing from output"
        );
    }

    #[test]
    fn test_encode_returns_valid_openmetrics() {
        let metrics = TiramiMetrics::new();
        let output = metrics.encode().unwrap();
        // Prometheus text format always includes # HELP and # TYPE lines.
        assert!(output.contains("# HELP"), "missing # HELP");
        assert!(output.contains("# TYPE"), "missing # TYPE");
    }

    #[test]
    fn test_observe_global_trade_count_gauge() {
        let mut ledger = ComputeLedger::new();
        let now_ms: u64 = 1_700_000_000_000;
        for i in 0..5u64 {
            ledger.trade_log.push(make_trade(
                [1u8; 32],
                [2u8; 32],
                50,
                now_ms - i * 1_000,
            ));
        }
        let metrics = TiramiMetrics::new();
        metrics.observe(&ledger, now_ms);
        // trade_count should have been incremented to 5.
        assert_eq!(metrics.trade_count.get(), 5);
    }

    #[test]
    fn test_tokenomics_gauges_populated_at_genesis() {
        let ledger = ComputeLedger::new();
        let metrics = TiramiMetrics::new();
        metrics.observe(&ledger, 1_700_000_000_000);
        // At genesis: total_minted=0, supply_factor=1.0, epoch=0, yield=0.001
        assert_eq!(metrics.total_minted.get(), 0);
        assert!((metrics.supply_factor.get() - 1.0).abs() < 1e-9);
        assert_eq!(metrics.current_epoch.get(), 0);
        assert!((metrics.yield_rate.get() - 0.001).abs() < 1e-9);
    }

    #[test]
    fn test_tokenomics_staking_gauges_with_pool() {
        let ledger = ComputeLedger::new();
        let mut pool = crate::staking::StakingPool::new();
        let node = NodeId([5u8; 32]);
        let now_ms: u64 = 1_000_000_000;
        pool.stake(node, 10_000, crate::staking::StakeDuration::Days90, now_ms).unwrap();
        let metrics = TiramiMetrics::new();
        metrics.observe_with_tokenomics(&ledger, now_ms, Some(&pool), None);
        assert_eq!(metrics.total_staked.get(), 10_000);
        assert_eq!(metrics.total_burned.get(), 0);
    }

    #[test]
    fn test_tokenomics_referral_gauge_with_tracker() {
        let ledger = ComputeLedger::new();
        let mut tracker = crate::referral::ReferralTracker::new();
        let sponsor = NodeId([1u8; 32]);
        let referred = NodeId([2u8; 32]);
        let now_ms: u64 = 1_000_000_000;
        tracker.register(sponsor, referred.clone(), now_ms).unwrap();
        tracker.mark_loan_repaid(&referred);
        tracker.mark_earn_threshold(&referred);
        // total_bonus_minted = REFERRAL_BONUS_TRM = 100
        let metrics = TiramiMetrics::new();
        metrics.observe_with_tokenomics(&ledger, now_ms, None, Some(&tracker));
        assert_eq!(metrics.referral_bonus_minted.get(), crate::referral::REFERRAL_BONUS_TRM as i64);
    }

    #[test]
    fn test_governance_gauges_with_state() {
        let ledger = ComputeLedger::new();
        let mut gov = crate::governance::GovernanceState::new(0);
        let proposer = NodeId([1u8; 32]);
        let now_ms = 1_000_000_000u64;
        let deadline = now_ms + 86_400_000;
        let pid = gov.create_proposal(
            proposer,
            crate::governance::ProposalKind::EmergencyPause,
            now_ms,
            deadline,
        ).unwrap();
        let voter = NodeId([2u8; 32]);
        gov.cast_vote(voter, pid, true, 5000, 0.9, 2).unwrap();
        let metrics = TiramiMetrics::new();
        metrics.observe_full(&ledger, now_ms, None, None, Some(&gov));
        assert_eq!(metrics.active_proposals.get(), 1);
        assert_eq!(metrics.total_votes.get(), 1);
    }

    #[test]
    fn test_governance_gauges_appear_in_encoded_output() {
        let metrics = TiramiMetrics::new();
        metrics.active_proposals.set(3);
        metrics.total_votes.set(42);
        let output = metrics.encode().unwrap();
        assert!(output.contains("tirami_active_proposals"), "missing tirami_active_proposals");
        assert!(output.contains("tirami_total_votes"), "missing tirami_total_votes");
    }

    #[test]
    fn test_tokenomics_metrics_appear_in_encoded_output() {
        let ledger = ComputeLedger::new();
        let metrics = TiramiMetrics::new();
        metrics.observe(&ledger, 1_700_000_000_000);
        let output = metrics.encode().unwrap();
        assert!(output.contains("tirami_total_minted"), "missing tirami_total_minted");
        assert!(output.contains("tirami_supply_factor"), "missing tirami_supply_factor");
        assert!(output.contains("tirami_current_epoch"), "missing tirami_current_epoch");
        assert!(output.contains("tirami_yield_rate"), "missing tirami_yield_rate");
        assert!(output.contains("tirami_total_staked"), "missing tirami_total_staked");
        assert!(output.contains("tirami_total_burned"), "missing tirami_total_burned");
        assert!(output.contains("tirami_referral_bonus_minted"), "missing tirami_referral_bonus_minted");
    }
}
