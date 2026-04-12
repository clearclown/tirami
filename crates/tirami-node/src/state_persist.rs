//! JSON persistence for L2/L3/L4 state (BankServices, Marketplace, TiramiMindAgent).
//!
//! Simple, unencrypted, integrity-less. HMAC-SHA256 can be added later if needed.
//! Phase 10 TODO: add HMAC-SHA256 integrity check if tampering becomes a concern.
//!
//! Returns `Ok(None)` when the file does not exist — a first-run case — and
//! `Err(io::Error)` only for real I/O or JSON parse failures.

use std::fs;
use std::io;
use std::path::Path;

use tirami_agora::{AgentRegistry, Marketplace};
use tirami_mind::{TiramiMindAgent, MindAgentSnapshot};

use crate::bank_adapter::{BankServices, BankServicesSnapshot};

// ---------------------------------------------------------------------------
// BankServices
// ---------------------------------------------------------------------------

/// Persist the current `BankServices` state to `path` as compact JSON.
pub fn save_bank(services: &BankServices, path: &Path) -> io::Result<()> {
    let snap = services.snapshot();
    let json = serde_json::to_string(&snap)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

/// Load `BankServices` from `path`.
///
/// Returns `Ok(None)` if the file does not exist.
/// Returns `Err` for I/O errors or JSON/validation failures.
pub fn load_bank(path: &Path) -> io::Result<Option<BankServices>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let snap: BankServicesSnapshot = serde_json::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let services = BankServices::from_snapshot(snap)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    Ok(Some(services))
}

// ---------------------------------------------------------------------------
// Marketplace
// ---------------------------------------------------------------------------

/// Persist the current `Marketplace` registry snapshot to `path` as compact JSON.
pub fn save_marketplace(mp: &Marketplace, path: &Path) -> io::Result<()> {
    let snap = mp.registry.snapshot();
    let json = serde_json::to_string(&snap)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

/// Load a `Marketplace` from `path`.
///
/// Returns `Ok(None)` if the file does not exist.
pub fn load_marketplace(path: &Path) -> io::Result<Option<Marketplace>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let snap: tirami_agora::registry::RegistrySnapshot = serde_json::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let registry = AgentRegistry::restore(snap);
    let mp = Marketplace {
        registry,
        calculator: tirami_agora::ReputationCalculator,
        matcher: tirami_agora::CapabilityMatcher,
    };
    Ok(Some(mp))
}

// ---------------------------------------------------------------------------
// TiramiMindAgent snapshot
// ---------------------------------------------------------------------------

/// Persist a `MindAgentSnapshot` extracted from a live `TiramiMindAgent`.
///
/// The optimizer and benchmark are NOT included in the snapshot.
pub fn save_mind(agent: &TiramiMindAgent, path: &Path) -> io::Result<()> {
    let snap = agent.snapshot();
    let json = serde_json::to_string(&snap)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

/// Load a `MindAgentSnapshot` from `path`.
///
/// Returns `Ok(None)` if the file does not exist.
/// The caller is responsible for re-attaching optimizer + benchmark via
/// `TiramiMindAgent::restore_from_snapshot()`.
pub fn load_mind_snapshot(path: &Path) -> io::Result<Option<MindAgentSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let snap: MindAgentSnapshot = serde_json::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(snap))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_agora::types::{AgentProfile, ModelTier};
    use tirami_mind::{EchoMetaOptimizer, Harness, InMemoryBenchmark};

    fn tmp_path(suffix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "forge_state_persist_test_{}_{}.json",
            std::process::id(),
            suffix
        ))
    }

    // ---------------------------------------------------------------------------
    // BankServices round-trip
    // ---------------------------------------------------------------------------

    #[test]
    fn test_bank_round_trip() {
        let path = tmp_path("bank");
        let _ = std::fs::remove_file(&path);

        let services = BankServices::new_default();
        save_bank(&services, &path).expect("save_bank failed");

        let loaded = load_bank(&path)
            .expect("load_bank returned Err")
            .expect("load_bank returned None");

        assert_eq!(
            loaded.portfolio.portfolio.cash_trm,
            services.portfolio.portfolio.cash_trm
        );
        assert_eq!(loaded.futures.len(), services.futures.len());
        assert_eq!(loaded.strategy_kind, services.strategy_kind);

        let _ = std::fs::remove_file(&path);
    }

    // ---------------------------------------------------------------------------
    // Marketplace round-trip
    // ---------------------------------------------------------------------------

    fn hex64(seed: &str) -> String {
        seed.repeat(64).chars().take(64).collect()
    }

    #[test]
    fn test_marketplace_round_trip() {
        let path = tmp_path("marketplace");
        let _ = std::fs::remove_file(&path);

        let mut mp = Marketplace::new();
        mp.register_agent(AgentProfile {
            agent_hex: hex64("a"),
            models_served: vec!["qwen3-8b".to_string()],
            trm_per_token: 3,
            tier: ModelTier::Medium,
            last_seen_ms: 1_700_000_000_000,
        });
        mp.register_agent(AgentProfile {
            agent_hex: hex64("b"),
            models_served: vec!["llama-3-8b".to_string()],
            trm_per_token: 5,
            tier: ModelTier::Medium,
            last_seen_ms: 1_700_000_000_001,
        });

        save_marketplace(&mp, &path).expect("save_marketplace failed");

        let loaded = load_marketplace(&path)
            .expect("load_marketplace returned Err")
            .expect("load_marketplace returned None");

        assert_eq!(loaded.registry.profile_count(), 2);
        let agent_a = loaded.registry.get_agent(&hex64("a")).unwrap();
        assert_eq!(agent_a.trm_per_token, 3);
        assert_eq!(agent_a.tier, ModelTier::Medium);

        let _ = std::fs::remove_file(&path);
    }

    // ---------------------------------------------------------------------------
    // MindAgentSnapshot round-trip
    // ---------------------------------------------------------------------------

    #[test]
    fn test_mind_snapshot_round_trip() {
        let path = tmp_path("mind");
        let _ = std::fs::remove_file(&path);

        let harness = Harness::new("test system prompt".to_string());
        let bench = InMemoryBenchmark::with_fn(|_| 0.5_f64);
        let opt = EchoMetaOptimizer;
        let agent = TiramiMindAgent::new(harness, Box::new(bench), Box::new(opt), None);

        save_mind(&agent, &path).expect("save_mind failed");

        let snap = load_mind_snapshot(&path)
            .expect("load_mind_snapshot returned Err")
            .expect("load_mind_snapshot returned None");

        assert_eq!(snap.harness.system_prompt, "test system prompt");
        assert_eq!(snap.harness.version, agent.harness.version);
        assert_eq!(snap.history.len(), agent.history().len());
        assert_eq!(snap.budget.max_trm_per_cycle, agent.runner_budget().max_trm_per_cycle);

        // Verify restore_from_snapshot works
        let harness2 = Harness::new("fresh start".to_string());
        let bench2 = InMemoryBenchmark::with_fn(|_| 0.5_f64);
        let opt2 = EchoMetaOptimizer;
        let mut agent2 = TiramiMindAgent::new(harness2, Box::new(bench2), Box::new(opt2), None);
        agent2.restore_from_snapshot(snap);
        assert_eq!(agent2.harness.system_prompt, "test system prompt");

        let _ = std::fs::remove_file(&path);
    }

    // ---------------------------------------------------------------------------
    // Missing file returns None
    // ---------------------------------------------------------------------------

    #[test]
    fn test_load_missing_file_returns_none() {
        let nonexistent = tmp_path("nonexistent_xyz_12345");
        // Ensure it really does not exist
        let _ = std::fs::remove_file(&nonexistent);

        assert!(load_bank(&nonexistent).unwrap().is_none());
        assert!(load_marketplace(&nonexistent).unwrap().is_none());
        assert!(load_mind_snapshot(&nonexistent).unwrap().is_none());
    }

    // ===========================================================================
    // DEEP SECURITY TESTS — Round 2 (corrupt JSON, empty files, garbage bytes)
    // ===========================================================================

    #[test]
    fn sec_deep_load_bank_from_corrupt_json_returns_err() {
        let path = tmp_path("corrupt_bank");
        std::fs::write(&path, "{invalid json!!!").unwrap();

        let result = load_bank(&path);
        assert!(
            result.is_err(),
            "corrupt JSON for bank state must return Err, not panic"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sec_deep_load_marketplace_from_empty_file_returns_err() {
        let path = tmp_path("empty_marketplace");
        std::fs::write(&path, "").unwrap();

        let result = load_marketplace(&path);
        assert!(
            result.is_err(),
            "empty file for marketplace state must return Err, not None"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sec_deep_load_mind_from_binary_garbage_returns_err() {
        let path = tmp_path("garbage_mind");
        // Write random binary garbage.
        let garbage: Vec<u8> = (0u8..=255u8).cycle().take(512).collect();
        std::fs::write(&path, &garbage).unwrap();

        let result = load_mind_snapshot(&path);
        assert!(
            result.is_err(),
            "binary garbage for mind state must return Err, not panic"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sec_deep_load_bank_from_valid_json_wrong_schema_returns_err() {
        // Valid JSON but wrong schema (missing required fields).
        let path = tmp_path("wrong_schema_bank");
        std::fs::write(&path, r#"{"foo": "bar", "baz": 42}"#).unwrap();

        let result = load_bank(&path);
        assert!(
            result.is_err(),
            "wrong-schema JSON for bank state must return Err"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sec_deep_load_marketplace_from_corrupt_json_returns_err() {
        let path = tmp_path("corrupt_marketplace");
        std::fs::write(&path, r#"{"agents": [null, null, {"broken": true}], "incomplete"#).unwrap();

        let result = load_marketplace(&path);
        assert!(
            result.is_err(),
            "corrupt JSON for marketplace must return Err"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sec_deep_mind_snapshot_with_empty_history_roundtrip() {
        let path = tmp_path("mind_empty_history");
        let _ = std::fs::remove_file(&path);

        let harness = Harness::new("empty history test".to_string());
        let bench = InMemoryBenchmark::with_fn(|_| 0.5_f64);
        let opt = EchoMetaOptimizer;
        let agent = TiramiMindAgent::new(harness, Box::new(bench), Box::new(opt), None);

        // Agent with no improvement cycles has empty history.
        assert_eq!(agent.history().len(), 0, "fresh agent must have empty history");

        save_mind(&agent, &path).expect("save_mind must succeed");
        let snap = load_mind_snapshot(&path)
            .expect("load must succeed")
            .expect("file must exist");

        assert_eq!(snap.history.len(), 0, "empty history must roundtrip correctly");
        assert_eq!(snap.harness.system_prompt, "empty history test");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sec_deep_strategy_kind_serialize_deserialize_all_variants() {
        use tirami_bank::StrategyKind;
        use serde_json;

        for kind in [
            StrategyKind::Conservative { max_commit_fraction: 0.30 },
            StrategyKind::HighYield { base_commit_fraction: 0.50 },
            StrategyKind::Balanced { threshold: 0.50 },
        ] {
            let json = serde_json::to_string(&kind).expect("must serialize");
            let roundtripped: StrategyKind = serde_json::from_str(&json).expect("must deserialize");
            assert_eq!(kind, roundtripped, "StrategyKind must roundtrip via JSON");
        }
    }
}
