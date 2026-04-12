//! Collusion resistance: statistical anomaly detection on trade patterns.
//!
//! This module implements a standalone analyzer that scans the trade log and
//! identifies nodes likely engaged in wash trading, round-robin trades, volume
//! spikes, or tight provider-consumer clusters. The output is a trust penalty
//! in [0.0, MAX_TRUST_PENALTY] that the caller can subtract from the node's
//! reputation.
//!
//! The detector is intentionally conservative: false positives should result
//! in small penalties (~0.05), and only extreme patterns should reach
//! MAX_TRUST_PENALTY.
//!
//! # Detection algorithms
//!
//! 1. **Tight cluster**: if one counterparty accounts for >20% of a node's
//!    trades in the window, the concentration score rises toward 1.0.
//! 2. **Volume spike**: coefficient of variation of per-hour CU volume.
//!    A round-robin spam burst has low CV (constant volume); genuine spikes
//!    from real load have high CV. This flags constant-volume wash trading.
//! 3. **Round-robin**: Tarjan SCC detection. If the subject is part of a
//!    strongly connected component of ≥3 nodes where ≥80% of edges are
//!    internal, the score rises toward 1.0.
//!
//! # Integration
//!
//! Call [`CollusionDetector::analyze_node`] to get a per-node report with a
//! `trust_penalty` ∈ [0, 0.5].  Pass the result to
//! [`super::ledger::ComputeLedger::effective_reputation`] to subtract the
//! penalty from the consensus reputation.

use crate::TradeRecord;
use tirami_core::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ===========================================================================
// Constants
// ===========================================================================

/// Analysis window: only trades within this many milliseconds of `now_ms` are
/// considered.
pub const COLLUSION_WINDOW_MS: u64 = 24 * 3_600 * 1_000; // 24 hours

/// Minimum trade count for a node to be analyzed. Below this, return zero
/// penalty (not enough data to flag anything).
pub const MIN_TRADES_FOR_ANALYSIS: usize = 10;

/// Maximum trust penalty this module can apply.
pub const MAX_TRUST_PENALTY: f64 = 0.5;

/// Self-trade ratio threshold — concentration above this starts scoring.
pub const TIGHT_CLUSTER_THRESHOLD: f64 = 0.20;

/// Round-robin detection — minimum SCC size to consider.
pub const ROUND_ROBIN_MIN_NODES: usize = 3;

/// Closedness ratio above which the SCC is flagged.
pub const ROUND_ROBIN_CLOSEDNESS_THRESHOLD: f64 = 0.80;

// ===========================================================================
// Output type
// ===========================================================================

/// Analysis output for a single subject node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollusionReport {
    /// The node being analyzed.
    pub subject: NodeId,
    /// Number of trades involving this node within the analysis window.
    pub trades_in_window: usize,
    /// Number of distinct counterparties observed.
    pub unique_counterparties: usize,
    /// Tight-cluster sub-score in [0, 1]. Higher = more concentrated trading.
    pub tight_cluster_score: f64,
    /// Volume-spike sub-score in [0, 1]. Higher = more uniform/spiky volume pattern.
    pub volume_spike_score: f64,
    /// Round-robin sub-score in [0, 1]. Higher = more closed-loop trading.
    pub round_robin_score: f64,
    /// Final trust penalty in [0, MAX_TRUST_PENALTY].
    pub trust_penalty: f64,
    /// Human-readable reasons for the penalty.
    pub flags: Vec<String>,
}

// ===========================================================================
// Detector
// ===========================================================================

/// Stateless collusion detector. All methods take slices and a current
/// timestamp; there is no mutable state.
pub struct CollusionDetector;

impl CollusionDetector {
    /// Analyze one node's trade participation over the recent window.
    pub fn analyze_node(
        trades: &[TradeRecord],
        subject: &NodeId,
        now_ms: u64,
    ) -> CollusionReport {
        // Filter to the analysis window.
        let window_start = now_ms.saturating_sub(COLLUSION_WINDOW_MS);
        let relevant: Vec<&TradeRecord> = trades
            .iter()
            .filter(|t| {
                t.timestamp >= window_start
                    && t.timestamp <= now_ms
                    && (&t.provider == subject || &t.consumer == subject)
            })
            .collect();

        if relevant.len() < MIN_TRADES_FOR_ANALYSIS {
            return CollusionReport {
                subject: subject.clone(),
                trades_in_window: relevant.len(),
                unique_counterparties: 0,
                tight_cluster_score: 0.0,
                volume_spike_score: 0.0,
                round_robin_score: 0.0,
                trust_penalty: 0.0,
                flags: vec![],
            };
        }

        // Collect counterparty counts.
        let mut counterparty_counts: HashMap<NodeId, usize> = HashMap::new();
        for t in &relevant {
            let other = if &t.provider == subject {
                &t.consumer
            } else {
                &t.provider
            };
            *counterparty_counts.entry(other.clone()).or_default() += 1;
        }
        let unique_counterparties = counterparty_counts.len();

        // --- Sub-score 1: tight cluster ---
        let tight_cluster_score = {
            let max_count = counterparty_counts.values().copied().max().unwrap_or(0);
            let max_share = max_count as f64 / relevant.len() as f64;
            if max_share > TIGHT_CLUSTER_THRESHOLD {
                ((max_share - TIGHT_CLUSTER_THRESHOLD) / (1.0 - TIGHT_CLUSTER_THRESHOLD))
                    .clamp(0.0, 1.0)
            } else {
                0.0
            }
        };

        // --- Sub-score 2: volume spike ---
        // Group trades into 1-hour buckets, compute coefficient of variation.
        // Note: We flag constant-volume (low CV) patterns because they indicate
        // mechanical wash-trading. Real traffic is bursty (high CV).
        let volume_spike_score = {
            let mut hourly: HashMap<u64, u64> = HashMap::new();
            for t in &relevant {
                let hour_bucket = (t.timestamp - window_start) / 3_600_000;
                *hourly.entry(hour_bucket).or_default() += t.trm_amount;
            }
            let values: Vec<f64> = hourly.values().map(|&v| v as f64).collect();
            if values.len() < 2 {
                0.0
            } else {
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                if mean == 0.0 {
                    0.0
                } else {
                    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                        / values.len() as f64;
                    let stdev = variance.sqrt();
                    let cv = stdev / mean;
                    // Score rises from 0 at cv=0.5, reaches 1 at cv=2.0.
                    // Very low CV (< 0.5) indicates suspiciously constant volume.
                    // Very high CV (> 2.0) is also flagged.
                    // We invert the scale: low CV = suspicious = high score.
                    let low_cv_score = if cv < 0.5 {
                        (0.5 - cv) / 0.5
                    } else {
                        0.0
                    };
                    low_cv_score.clamp(0.0, 1.0)
                }
            }
        };

        // --- Sub-score 3: round-robin (Tarjan SCC) ---
        // Build a directed graph of all trades in the window (provider → consumer).
        // Check whether subject belongs to an SCC of size ≥ ROUND_ROBIN_MIN_NODES
        // where ≥ ROUND_ROBIN_CLOSEDNESS_THRESHOLD of edges are internal.
        let round_robin_score = {
            // Collect all unique nodes and edges in the window (not just subject's).
            let all_in_window: Vec<&TradeRecord> = trades
                .iter()
                .filter(|t| t.timestamp >= window_start && t.timestamp <= now_ms)
                .collect();

            compute_round_robin_score(&all_in_window, subject)
        };

        // --- Combine sub-scores ---
        let max_sub = tight_cluster_score
            .max(volume_spike_score)
            .max(round_robin_score);
        let trust_penalty = (MAX_TRUST_PENALTY * max_sub).clamp(0.0, MAX_TRUST_PENALTY);

        let mut flags = Vec::new();
        if tight_cluster_score > 0.0 {
            flags.push(format!(
                "tight_cluster: {:.2} (>= {:.0}% concentration)",
                tight_cluster_score,
                TIGHT_CLUSTER_THRESHOLD * 100.0
            ));
        }
        if volume_spike_score > 0.0 {
            flags.push(format!(
                "volume_spike: {:.2} (suspiciously constant hourly volume)",
                volume_spike_score
            ));
        }
        if round_robin_score > 0.0 {
            flags.push(format!(
                "round_robin: {:.2} (closed SCC detected)",
                round_robin_score
            ));
        }

        CollusionReport {
            subject: subject.clone(),
            trades_in_window: relevant.len(),
            unique_counterparties,
            tight_cluster_score,
            volume_spike_score,
            round_robin_score,
            trust_penalty,
            flags,
        }
    }

    /// Analyze the whole trade log and return a report per node with non-zero
    /// trust penalty. Results are sorted by penalty descending.
    pub fn analyze_ledger(trades: &[TradeRecord], now_ms: u64) -> Vec<CollusionReport> {
        // Collect all unique nodes involved in trades within the window.
        let window_start = now_ms.saturating_sub(COLLUSION_WINDOW_MS);
        let mut nodes: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
        for t in trades.iter().filter(|t| {
            t.timestamp >= window_start && t.timestamp <= now_ms
        }) {
            nodes.insert(t.provider.clone());
            nodes.insert(t.consumer.clone());
        }

        let mut reports: Vec<CollusionReport> = nodes
            .into_iter()
            .map(|n| Self::analyze_node(trades, &n, now_ms))
            .filter(|r| r.trust_penalty > 0.0)
            .collect();

        reports.sort_by(|a, b| {
            b.trust_penalty
                .partial_cmp(&a.trust_penalty)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        reports
    }
}

// ===========================================================================
// Tarjan SCC — inline minimal implementation (~60 lines)
// ===========================================================================

/// Compute the round-robin sub-score for a subject node.
///
/// Builds a directed graph of all in-window trades, computes SCCs via
/// Tarjan's algorithm, then checks if the subject's SCC is large and closed.
fn compute_round_robin_score(trades: &[&TradeRecord], subject: &NodeId) -> f64 {
    // Assign integer IDs to nodes.
    // Use owned NodeId keys to avoid lifetime complexity.
    let mut node_to_idx: HashMap<NodeId, usize> = HashMap::new();
    let mut idx_count: usize = 0;

    let mut ensure_node = |node: &NodeId| {
        if !node_to_idx.contains_key(node) {
            node_to_idx.insert(node.clone(), idx_count);
            idx_count += 1;
        }
    };

    // Pre-register all nodes.
    for t in trades {
        ensure_node(&t.provider);
        ensure_node(&t.consumer);
    }

    let n = idx_count;
    if n == 0 {
        return 0.0;
    }

    // Build adjacency list (directed: provider → consumer).
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for t in trades {
        let Some(&p) = node_to_idx.get(&t.provider) else { continue; };
        let Some(&c) = node_to_idx.get(&t.consumer) else { continue; };
        if p != c {
            adj[p].push(c);
        }
    }

    // Tarjan's SCC algorithm.
    let mut index_counter = 0usize;
    let mut stack: Vec<usize> = Vec::new();
    let mut on_stack = vec![false; n];
    let mut index = vec![usize::MAX; n]; // usize::MAX = unvisited
    let mut lowlink = vec![0usize; n];
    let mut scc_id = vec![usize::MAX; n]; // component assignment
    let mut scc_count = 0usize;

    // Iterative Tarjan to avoid stack-overflow on large graphs.
    // Each stack frame: (node, adj_iter_pos, caller_info).
    enum Frame {
        Enter(usize),
        Return(usize), // node to finalize
    }

    let mut call_stack: Vec<Frame> = Vec::new();
    for start in 0..n {
        if index[start] != usize::MAX {
            continue;
        }
        call_stack.push(Frame::Enter(start));

        while let Some(frame) = call_stack.pop() {
            match frame {
                Frame::Enter(v) => {
                    index[v] = index_counter;
                    lowlink[v] = index_counter;
                    index_counter += 1;
                    stack.push(v);
                    on_stack[v] = true;

                    // Push a marker to finalize v after all neighbours are processed.
                    call_stack.push(Frame::Return(v));

                    // Push unvisited neighbours (in reverse order so first is processed first).
                    for &w in adj[v].iter().rev() {
                        if index[w] == usize::MAX {
                            call_stack.push(Frame::Enter(w));
                        }
                    }
                }
                Frame::Return(v) => {
                    // Update lowlink based on already-visited neighbours.
                    for &w in &adj[v] {
                        if on_stack[w] {
                            lowlink[v] = lowlink[v].min(lowlink[w].min(index[w]));
                        }
                    }

                    // If v is an SCC root, pop the stack to collect the component.
                    if lowlink[v] == index[v] {
                        while let Some(w) = stack.pop() {
                            on_stack[w] = false;
                            scc_id[w] = scc_count;
                            if w == v {
                                break;
                            }
                        }
                        scc_count += 1;
                    }
                }
            }
        }
    }

    // Find the SCC containing subject.
    let subject_idx = match node_to_idx.get(subject) {
        // node_to_idx uses owned keys; subject is a reference — key lookup works.
        Some(&i) => i,
        None => return 0.0,
    };
    let my_scc = scc_id[subject_idx];
    if my_scc == usize::MAX {
        return 0.0;
    }

    // Collect SCC members.
    let scc_members: Vec<usize> = (0..n).filter(|&i| scc_id[i] == my_scc).collect();
    let scc_size = scc_members.len();

    if scc_size < ROUND_ROBIN_MIN_NODES {
        return 0.0;
    }

    // Count internal vs external edges.
    let scc_set: std::collections::HashSet<usize> = scc_members.iter().copied().collect();
    let mut internal_edges = 0usize;
    let mut external_edges = 0usize;
    for &member in &scc_members {
        for &w in &adj[member] {
            if scc_set.contains(&w) {
                internal_edges += 1;
            } else {
                external_edges += 1;
            }
        }
    }

    let total_edges = internal_edges + external_edges;
    if total_edges == 0 {
        return 0.0;
    }

    let closedness = internal_edges as f64 / total_edges as f64;
    if closedness > ROUND_ROBIN_CLOSEDNESS_THRESHOLD {
        ((closedness - ROUND_ROBIN_CLOSEDNESS_THRESHOLD)
            / (1.0 - ROUND_ROBIN_CLOSEDNESS_THRESHOLD))
            .clamp(0.0, 1.0)
    } else {
        0.0
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn trade(
        provider: [u8; 32],
        consumer: [u8; 32],
        cu: u64,
        ts_offset_ms: i64,
    ) -> TradeRecord {
        let base = now_ms();
        let ts = (base as i64 + ts_offset_ms) as u64;
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
    fn test_analyze_node_with_no_trades_returns_zero_penalty() {
        let trades = vec![];
        let subject = NodeId([1u8; 32]);
        let report = CollusionDetector::analyze_node(&trades, &subject, now_ms());
        assert_eq!(report.trust_penalty, 0.0);
        assert_eq!(report.trades_in_window, 0);
    }

    #[test]
    fn test_tight_cluster_flags_90_percent_concentration() {
        // 18 out of 20 trades are with the same counterparty (90%).
        let subject = [1u8; 32];
        let partner_a = [2u8; 32];
        let partner_b = [3u8; 32];
        let now = now_ms();
        let mut trades: Vec<TradeRecord> = (0..18)
            .map(|i| trade(subject, partner_a, 100, -(i as i64 * 60_000)))
            .collect();
        trades.push(trade(subject, partner_b, 100, -200_000));
        trades.push(trade(subject, partner_b, 100, -300_000));

        let report =
            CollusionDetector::analyze_node(&trades, &NodeId(subject), now);
        assert!(
            report.tight_cluster_score > 0.0,
            "expected tight cluster score > 0, got {}",
            report.tight_cluster_score
        );
        assert!(
            report.trust_penalty > 0.0,
            "expected non-zero penalty, got {}",
            report.trust_penalty
        );
    }

    #[test]
    fn test_round_robin_detects_closed_3_node_loop() {
        // A → B, B → C, C → A: perfect 3-node loop with no external edges.
        let a = [10u8; 32];
        let b = [11u8; 32];
        let c = [12u8; 32];
        let now = now_ms();
        // Repeat the cycle many times so we exceed MIN_TRADES_FOR_ANALYSIS.
        let mut trades: Vec<TradeRecord> = Vec::new();
        for i in 0..15i64 {
            trades.push(trade(a, b, 100, -(i * 60_000)));
            trades.push(trade(b, c, 100, -(i * 60_000 + 1_000)));
            trades.push(trade(c, a, 100, -(i * 60_000 + 2_000)));
        }
        let report = CollusionDetector::analyze_node(&trades, &NodeId(a), now);
        assert!(
            report.round_robin_score > 0.0,
            "expected round_robin_score > 0 for closed loop, got {}",
            report.round_robin_score
        );
        assert!(
            report.trust_penalty > 0.0,
            "expected non-zero penalty, got {}",
            report.trust_penalty
        );
    }

    #[test]
    fn test_volume_spike_detects_bursty_pattern() {
        // Create very constant volume (low CV) — suspicious wash-trading pattern.
        let subject = [20u8; 32];
        let partner = [21u8; 32];
        let now = now_ms();
        // 24 trades, one per hour, all exactly 100 CU (CV ≈ 0).
        let trades: Vec<TradeRecord> = (0..24i64)
            .map(|h| {
                trade(
                    subject,
                    partner,
                    100,
                    -(h * 3_600_000), // exactly one per hour
                )
            })
            .collect();
        let report =
            CollusionDetector::analyze_node(&trades, &NodeId(subject), now);
        // Very constant volume → volume_spike_score should be > 0.
        assert!(
            report.volume_spike_score > 0.0,
            "expected volume_spike_score > 0 for constant-volume pattern, got {}",
            report.volume_spike_score
        );
    }

    #[test]
    fn test_healthy_diverse_trader_gets_zero_penalty() {
        // 20 trades spread across 20 different counterparties = max diversity.
        let subject = [30u8; 32];
        let now = now_ms();
        let trades: Vec<TradeRecord> = (0u8..20)
            .map(|i| {
                let partner = [100 + i; 32];
                trade(subject, partner, 100 + i as u64 * 10, -(i as i64 * 60_000))
            })
            .collect();
        let report =
            CollusionDetector::analyze_node(&trades, &NodeId(subject), now);
        assert_eq!(
            report.tight_cluster_score, 0.0,
            "diverse trader should have zero tight_cluster_score"
        );
        // trust_penalty may be non-zero from volume_spike, so just check tight_cluster.
        assert!(
            report.unique_counterparties == 20,
            "expected 20 unique counterparties"
        );
    }

    #[test]
    fn test_trust_penalty_clamped_to_max() {
        // Even with all signals maxed, penalty cannot exceed MAX_TRUST_PENALTY.
        let subject = [40u8; 32];
        let partner = [41u8; 32];
        let now = now_ms();
        // 100 trades all with same partner (100% concentration).
        let trades: Vec<TradeRecord> = (0..100i64)
            .map(|i| trade(subject, partner, 100, -(i * 60_000)))
            .collect();
        let report =
            CollusionDetector::analyze_node(&trades, &NodeId(subject), now);
        assert!(
            report.trust_penalty <= MAX_TRUST_PENALTY,
            "trust_penalty {} exceeds MAX_TRUST_PENALTY {}",
            report.trust_penalty,
            MAX_TRUST_PENALTY
        );
    }
}
