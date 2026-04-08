//! Prometheus / OpenMetrics export for CollusionReport + ledger telemetry.
//!
//! Phase 10 P5 stub — full implementation lands in the Phase 10 agent batch.
//! This module will expose a `MetricsRegistry` wrapping `prometheus::Registry`
//! with counters/gauges for the collusion detector and reputation subsystem.
#![allow(dead_code)]
