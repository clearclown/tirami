//! Prometheus / OpenMetrics export for Forge telemetry.
//!
//! Exposes:
//! - forge_cu_contributed_total   (gauge, per-node_id)
//! - forge_cu_consumed_total      (gauge, per-node_id)
//! - forge_reputation             (gauge, per-node_id)
//! - forge_trade_count_total      (counter, global)
//! - forge_active_loan_count      (gauge, global)
//! - forge_pool_total_trm          (gauge, global)
//! - forge_pool_reserve_ratio     (gauge, global)
//! - forge_collusion_tight_cluster_score  (gauge, per-node_id)
//! - forge_collusion_volume_spike_score   (gauge, per-node_id)
//! - forge_collusion_round_robin_score    (gauge, per-node_id)
//! - forge_collusion_trust_penalty        (gauge, per-node_id)

use crate::collusion::CollusionDetector;
use crate::ledger::ComputeLedger;
use prometheus::{
    Encoder, Gauge, GaugeVec, IntCounter, IntGauge, Opts, Registry, TextEncoder,
};

/// Holds all Prometheus metrics for a Forge node.
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
}

impl TiramiMetrics {
    /// Create a new TiramiMetrics with its own isolated Registry.
    pub fn new() -> Self {
        let registry = Registry::new();

        let cu_contributed = GaugeVec::new(
            Opts::new(
                "forge_cu_contributed_total",
                "Total CU contributed (earned) by a node",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let cu_consumed = GaugeVec::new(
            Opts::new(
                "forge_cu_consumed_total",
                "Total CU consumed (spent) by a node",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let reputation = GaugeVec::new(
            Opts::new("forge_reputation", "Reputation score for a node (0.0–1.0)"),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let trade_count = IntCounter::with_opts(Opts::new(
            "forge_trade_count_total",
            "Total number of trades recorded in the ledger",
        ))
        .expect("valid counter opts");

        let active_loan_count = IntGauge::with_opts(Opts::new(
            "forge_active_loan_count",
            "Number of currently active loans in the lending pool",
        ))
        .expect("valid gauge opts");

        let pool_total_trm = IntGauge::with_opts(Opts::new(
            "forge_pool_total_trm",
            "Total CU deposited into the lending pool",
        ))
        .expect("valid gauge opts");

        let pool_reserve_ratio = Gauge::with_opts(Opts::new(
            "forge_pool_reserve_ratio",
            "Lending pool reserve ratio (available / total)",
        ))
        .expect("valid gauge opts");

        let collusion_tight = GaugeVec::new(
            Opts::new(
                "forge_collusion_tight_cluster_score",
                "Tight-cluster collusion sub-score for a node (0.0–1.0)",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let collusion_spike = GaugeVec::new(
            Opts::new(
                "forge_collusion_volume_spike_score",
                "Volume-spike collusion sub-score for a node (0.0–1.0)",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let collusion_robin = GaugeVec::new(
            Opts::new(
                "forge_collusion_round_robin_score",
                "Round-robin collusion sub-score for a node (0.0–1.0)",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

        let collusion_penalty = GaugeVec::new(
            Opts::new(
                "forge_collusion_trust_penalty",
                "Final trust penalty applied to node reputation (0.0–0.5)",
            ),
            &["node_id"],
        )
        .expect("valid gauge vec opts");

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
        }
    }

    /// Snapshot ledger state into all metrics.
    ///
    /// This is idempotent and cheap to call — Prometheus gauges are set (not
    /// incremented), so calling `observe` twice in a row is safe.
    pub fn observe(&self, ledger: &ComputeLedger, now_ms: u64) {
        // Per-node balance and reputation metrics.
        for balance in ledger.balances.values() {
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
            output.contains("forge_cu_contributed_total"),
            "missing forge_cu_contributed_total in:\n{output}"
        );
        assert!(
            output.contains("forge_reputation"),
            "missing forge_reputation"
        );
        // collusion gauges only appear if trades exist; check a global metric instead.
        assert!(
            output.contains("forge_trade_count_total"),
            "missing forge_trade_count_total"
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
        assert!(output.contains("forge_reputation"), "forge_reputation missing");
        assert!(
            output.contains(&node.to_hex()),
            "node hex {} not in output",
            node.to_hex()
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
            output.contains("forge_collusion_tight_cluster_score"),
            "tight_cluster metric missing"
        );
        assert!(
            output.contains("forge_collusion_trust_penalty"),
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
}
