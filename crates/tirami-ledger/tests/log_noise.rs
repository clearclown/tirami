//! Measures how often steady-state economic verdicts emit WARN (#150, #153).
//!
//! Both issues report the same shape: a decision the protocol is *supposed* to
//! make, logged at WARN on every occurrence, so log volume scales with retry
//! rate rather than with anything an operator can act on. These tests count
//! the emissions rather than eyeballing a log file.

use std::sync::{Arc, Mutex};
use tirami_core::NodeId;
use tirami_ledger::ComputeLedger;
use tracing::Level;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

#[derive(Default, Debug)]
struct Counts {
    warn: usize,
    debug: usize,
    messages: Vec<String>,
}

#[derive(Clone, Default)]
struct CountingLayer(Arc<Mutex<Counts>>);

struct MessageVisitor(String);
impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for CountingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        let mut counts = self.0.lock().unwrap();
        match *event.metadata().level() {
            Level::WARN => {
                counts.warn += 1;
                counts.messages.push(visitor.0);
            }
            Level::DEBUG => counts.debug += 1,
            _ => {}
        }
    }
}

/// Drive `can_afford` from `calls` distinct unknown nodes against a ledger that
/// already holds `free_tier_nodes` free-tier-only balances, and report what got
/// logged.
fn measure(free_tier_nodes: usize, calls: usize) -> Counts {
    let layer = CountingLayer::default();
    let sink = layer.0.clone();

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let mut ledger = ComputeLedger::new();

        // A free-tier-only node is one that consumed without ever contributing.
        for i in 0..free_tier_nodes {
            let mut id = [0u8; 32];
            id[0..8].copy_from_slice(&(i as u64).to_be_bytes());
            ledger.record_consumption(&NodeId(id), 1);
        }

        for i in 0..calls {
            let mut id = [0xEEu8; 32];
            id[0..8].copy_from_slice(&(i as u64).to_be_bytes());
            let _ = ledger.can_afford(&NodeId(id), 1);
        }
    });

    Arc::try_unwrap(sink).unwrap().into_inner().unwrap()
}

/// #153 — every rejected request used to re-fire the same WARN, so volume was
/// 1:1 with retry rate. Measured before the fix: 100 rejections, 100 WARN
/// lines. Being at the cap is a state, so it is announced once.
#[test]
fn sybil_rejection_warns_once_per_episode_not_per_request() {
    let few = measure(51, 10);
    let many = measure(51, 1_000);

    assert_eq!(
        few.warn, 1,
        "entering the capped state should be announced exactly once, got {:?}",
        few.messages
    );
    assert_eq!(
        many.warn, few.warn,
        "100x the requests must not mean 100x the WARN lines — that is the bug \
         (#153). 1000 calls produced {} WARN lines.",
        many.warn
    );

    // The individual rejections stay observable, just not at WARN.
    assert_eq!(
        many.debug, 999,
        "every rejection after the first should be visible at DEBUG"
    );
}

/// Below the cap nothing should be logged at all: this is the ordinary path.
#[test]
fn a_healthy_mesh_logs_nothing() {
    let counts = measure(3, 20);
    assert_eq!(counts.warn, 0);
    assert_eq!(counts.debug, 0);
}
