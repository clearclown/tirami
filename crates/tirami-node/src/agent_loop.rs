//! Phase 18.5-part-2 — PersonalAgent background tick loop.
//!
//! Periodically samples node state, builds a
//! [`tirami_mind::TickContext`], and calls
//! [`PersonalAgent::tick`]. The returned [`tirami_mind::TickAction`]
//! is either applied in-place (e.g. `ResetDailyTally`) or surfaced
//! via `tracing` for later wiring into the real execution path
//! (HTTP → provider, serving requests, etc.).
//!
//! The loop is deliberately thin: `tick` is a pure function, so the
//! hard work is building the context. Keeping the dispatch minimal
//! here means the decision policy lives in one place
//! (`tirami_mind::personal_agent::tick`) and is fully unit-testable
//! without a running node.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tirami_mind::{PersonalAgent, TickAction, TickContext};
use tokio::sync::Mutex;

/// Observability counters exposed so `/v1/tirami/agent/status`
/// (and tests) can show how the loop is behaving without having to
/// read log lines.
#[derive(Debug, Clone, Default)]
pub struct AgentLoopStats {
    /// Total ticks dispatched since the node started.
    pub ticks: u64,
    /// Tag for the most recent action (see [`action_kind`]).
    pub last_action: Option<&'static str>,
    /// unix-ms of the most recent tick, or 0 if never ticked.
    pub last_tick_ms: u64,
}

impl AgentLoopStats {
    pub fn new() -> Self {
        Self::default()
    }
}

/// External signals the loop needs but cannot derive on its own
/// (utilization sampling, task queue peek, peer request intake).
/// Kept as a plain struct so the caller can rebuild it each tick
/// with fresh samples.
#[derive(Debug, Clone, Default)]
pub struct AgentTickInput {
    /// Current CPU/GPU utilization in `[0.0, 1.0]`. Default 0.0 in
    /// the scaffold because we don't yet sample real load.
    pub local_utilization: f64,
    /// Seconds the machine has been continuously idle.
    pub seconds_idle: u64,
    /// Agent's current TRM balance (for the approval path).
    pub current_balance_trm: u64,
    /// Pending user task + id, when one was just enqueued.
    pub pending_task: Option<(String, tirami_mind::TaskCostEstimate)>,
    /// Peer serving request that just arrived.
    pub incoming_serving_request: Option<tirami_mind::ServingRequest>,
}

/// Short string used in `AgentLoopStats.last_action` and in logs.
/// Kept as `&'static str` so it can live in `Option<&str>` without
/// allocation churn.
fn action_kind(action: &TickAction) -> &'static str {
    match action {
        TickAction::Idle => "idle",
        TickAction::ResetDailyTally => "reset_daily",
        TickAction::ServeRequest { .. } => "serve",
        TickAction::RejectServeRequest { .. } => "reject_serve",
        TickAction::RunLocal { .. } => "run_local",
        TickAction::RunRemote { .. } => "run_remote",
        TickAction::AskUser { .. } => "ask_user",
        TickAction::StartEarning => "start_earning",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Run the tick logic once against a personal-agent slot. If the
/// slot is empty, the call is a no-op and `stats.ticks` is left
/// untouched (we don't count "no-op" ticks as real work).
///
/// Exposed (non-`pub` outside the crate) so unit tests can drive it
/// deterministically without spawning the tokio loop.
pub(crate) async fn run_tick_once(
    agent_slot: &Arc<Mutex<Option<PersonalAgent>>>,
    stats: &Arc<Mutex<AgentLoopStats>>,
    input: &AgentTickInput,
) {
    let tick_ms = now_ms();
    let mut guard = agent_slot.lock().await;
    let Some(agent) = guard.as_mut() else {
        return;
    };
    let ctx = TickContext {
        now_ms: tick_ms,
        local_utilization: input.local_utilization,
        seconds_idle: input.seconds_idle,
        pending_task: input.pending_task.as_ref().map(|(_, t)| t.clone()),
        pending_task_id: input.pending_task.as_ref().map(|(id, _)| id.clone()),
        current_balance_trm: input.current_balance_trm,
        incoming_serving_request: input.incoming_serving_request.clone(),
    };
    let action = agent.tick(&ctx);
    let kind = action_kind(&action);

    // Apply the pieces of the action the agent can handle on its
    // own. Anything that requires an HTTP call / provider selection
    // is left to the pipeline (surfaced via tracing for now).
    match &action {
        TickAction::ResetDailyTally => {
            agent.reset_daily_tally(tick_ms);
        }
        TickAction::Idle | TickAction::StartEarning => {
            // Nothing to persist on the agent side. Real earning
            // happens when a peer hits the chat endpoint; the
            // loop's role is just to say "we're open for business".
        }
        TickAction::ServeRequest { .. }
        | TickAction::RejectServeRequest { .. }
        | TickAction::RunLocal { .. }
        | TickAction::RunRemote { .. }
        | TickAction::AskUser { .. } => {
            // Dispatch is the caller's responsibility (pipeline /
            // API handler). The loop only records the decision.
        }
    }
    drop(guard);

    let mut stats_guard = stats.lock().await;
    stats_guard.ticks = stats_guard.ticks.saturating_add(1);
    stats_guard.last_action = Some(kind);
    stats_guard.last_tick_ms = tick_ms;
    drop(stats_guard);

    tracing::debug!(
        target: "tirami_node::agent_loop",
        kind,
        "agent tick dispatched"
    );
}

/// Spawn the background tick loop as a tokio task. Returns the
/// JoinHandle so the caller can `abort()` it on shutdown. The loop
/// always ticks on `interval_secs` regardless of whether the agent
/// slot is populated — that way, the moment the operator configures
/// an agent, the loop starts acting on it without needing a restart.
pub fn spawn_agent_tick_loop(
    agent_slot: Arc<Mutex<Option<PersonalAgent>>>,
    stats: Arc<Mutex<AgentLoopStats>>,
    interval_secs: u64,
    input_sampler: impl Fn() -> AgentTickInput + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = if interval_secs == 0 { 1 } else { interval_secs };
        let mut ticker = tokio::time::interval(Duration::from_secs(interval));
        // Skip the first immediate fire so tests / startup don't
        // race with the slot being populated.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let input = input_sampler();
            run_tick_once(&agent_slot, &stats, &input).await;
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::NodeId;
    use tirami_mind::{AgentPreferences, PersonalAgent, ServingRequest, TaskCostEstimate, TaskSize};

    fn make_slot(agent: Option<PersonalAgent>) -> Arc<Mutex<Option<PersonalAgent>>> {
        Arc::new(Mutex::new(agent))
    }

    fn make_stats() -> Arc<Mutex<AgentLoopStats>> {
        Arc::new(Mutex::new(AgentLoopStats::new()))
    }

    fn default_agent() -> PersonalAgent {
        // Stamp the agent with "now" so the daily-rollover clock
        // doesn't fire on the first tick. Tests that *want* to
        // exercise the 24h path construct their own agent with
        // `day_started_at_ms = 0`.
        PersonalAgent::new(NodeId([0xAAu8; 32]), Default::default(), now_ms())
    }

    #[tokio::test]
    async fn no_op_when_slot_is_empty() {
        let slot = make_slot(None);
        let stats = make_stats();
        run_tick_once(&slot, &stats, &AgentTickInput::default()).await;
        let s = stats.lock().await;
        assert_eq!(s.ticks, 0);
        assert!(s.last_action.is_none());
    }

    #[tokio::test]
    async fn idle_tick_increments_counter() {
        let slot = make_slot(Some(default_agent()));
        let stats = make_stats();
        run_tick_once(&slot, &stats, &AgentTickInput::default()).await;
        let s = stats.lock().await;
        assert_eq!(s.ticks, 1);
        assert_eq!(s.last_action, Some("idle"));
    }

    #[tokio::test]
    async fn multiple_ticks_accumulate() {
        let slot = make_slot(Some(default_agent()));
        let stats = make_stats();
        for _ in 0..5 {
            run_tick_once(&slot, &stats, &AgentTickInput::default()).await;
        }
        let s = stats.lock().await;
        assert_eq!(s.ticks, 5);
    }

    #[tokio::test]
    async fn serve_request_is_tagged_in_stats() {
        let slot = make_slot(Some(default_agent()));
        let stats = make_stats();
        let input = AgentTickInput {
            incoming_serving_request: Some(ServingRequest {
                peer: NodeId([0x11u8; 32]),
                task_id: "srv-1".into(),
                peer_reputation: 0.9,
                prompt_passes_filter: true,
                estimated_reward_trm: 2,
            }),
            ..Default::default()
        };
        run_tick_once(&slot, &stats, &input).await;
        assert_eq!(stats.lock().await.last_action, Some("serve"));
    }

    #[tokio::test]
    async fn local_task_tags_run_local() {
        let slot = make_slot(Some(default_agent()));
        let stats = make_stats();
        let input = AgentTickInput {
            pending_task: Some((
                "task-local".into(),
                TaskCostEstimate {
                    size: TaskSize::Local,
                    estimated_trm: 0,
                    estimated_seconds: 1,
                    preferred_provider: None,
                },
            )),
            ..Default::default()
        };
        run_tick_once(&slot, &stats, &input).await;
        assert_eq!(stats.lock().await.last_action, Some("run_local"));
    }

    #[tokio::test]
    async fn ask_user_is_surfaced() {
        let slot = make_slot(Some(default_agent()));
        let stats = make_stats();
        let input = AgentTickInput {
            current_balance_trm: 1_000,
            pending_task: Some((
                "task-expensive".into(),
                TaskCostEstimate {
                    size: TaskSize::Remote,
                    estimated_trm: 100, // over per-task budget
                    estimated_seconds: 60,
                    preferred_provider: Some(NodeId([0x33u8; 32])),
                },
            )),
            ..Default::default()
        };
        run_tick_once(&slot, &stats, &input).await;
        assert_eq!(stats.lock().await.last_action, Some("ask_user"));
    }

    #[tokio::test]
    async fn start_earning_fires_when_idle_long_enough() {
        let slot = make_slot(Some(default_agent()));
        let stats = make_stats();
        let input = AgentTickInput {
            local_utilization: 0.01,
            seconds_idle: 1_000,
            ..Default::default()
        };
        run_tick_once(&slot, &stats, &input).await;
        assert_eq!(stats.lock().await.last_action, Some("start_earning"));
    }

    #[tokio::test]
    async fn reset_daily_tally_clears_agent_counters() {
        let mut agent =
            PersonalAgent::new(NodeId([0xAAu8; 32]), Default::default(), 0);
        agent.record_spend(5);
        agent.record_earn(10);
        let slot = make_slot(Some(agent));
        let stats = make_stats();

        // First tick happens "now" — which is >24h after
        // day_started_at_ms=0, so tick should request a reset and
        // apply it in-place.
        run_tick_once(&slot, &stats, &AgentTickInput::default()).await;
        assert_eq!(stats.lock().await.last_action, Some("reset_daily"));
        let guard = slot.lock().await;
        let agent = guard.as_ref().unwrap();
        assert_eq!(agent.spent_today_trm, 0);
        assert_eq!(agent.earned_today_trm, 0);
    }

    #[tokio::test]
    async fn spawn_loop_populates_stats_over_time() {
        // Use a 1-second interval (the smallest supported) and wait
        // slightly over two intervals to see at least one tick.
        let agent = PersonalAgent::new(NodeId([0xAAu8; 32]), Default::default(), now_ms());
        let slot = make_slot(Some(agent));
        let stats = make_stats();
        let handle = spawn_agent_tick_loop(
            slot.clone(),
            stats.clone(),
            1,
            || AgentTickInput::default(),
        );
        tokio::time::sleep(Duration::from_millis(2_300)).await;
        handle.abort();
        let s = stats.lock().await;
        assert!(s.ticks >= 1, "expected >=1 tick, got {}", s.ticks);
    }

    #[tokio::test]
    async fn preferences_flow_through_tick_loop() {
        // auto_earn=false means even a valid serving request should
        // come back as a rejection.
        let mut prefs = AgentPreferences::default();
        prefs.auto_earn_enabled = false;
        let agent = PersonalAgent::with_preferences(
            NodeId([0xAAu8; 32]),
            Default::default(),
            prefs,
            now_ms(),
        )
        .unwrap();
        let slot = make_slot(Some(agent));
        let stats = make_stats();
        let input = AgentTickInput {
            incoming_serving_request: Some(ServingRequest {
                peer: NodeId([0x11u8; 32]),
                task_id: "srv".into(),
                peer_reputation: 0.9,
                prompt_passes_filter: true,
                estimated_reward_trm: 1,
            }),
            ..Default::default()
        };
        run_tick_once(&slot, &stats, &input).await;
        assert_eq!(stats.lock().await.last_action, Some("reject_serve"));
    }
}
