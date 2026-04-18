//! Phase 17 Wave 2.2 — probabilistic "heavy" audit scaffold.
//!
//! # What this is
//!
//! A scaffold for a Filecoin-style periodic deep audit. The existing
//! Phase 14.3 challenge-response (1-on-1 hash comparison) is cheap
//! and runs on every eligible trade; this module adds a SECOND, rarer
//! audit mode that costs meaningfully more compute and thus provides
//! a stronger honesty signal when it fires:
//!
//! * Fires probabilistically (default 1 %) on completed trades.
//! * Selects N = 3 independent validators at random.
//! * Each validator re-runs the same deterministic input on its own
//!   loaded model.
//! * Outcome is by **2-of-3 quorum**:
//!   - All 3 agree → `QuorumVerdict::Passed`.
//!   - 2 of 3 agree → the dissenter is `QuorumVerdict::Dissenter`
//!     (slashable as `"audit-fail"` by the caller).
//!   - No 2-of-3 agreement → `QuorumVerdict::Inconclusive`
//!     (the audit is inconclusive; policy is "don't slash on
//!     ambiguous evidence" — the caller logs & moves on).
//!
//! # Why scaffold, not SNARK
//!
//! The plan wants to eventually compress this with a SNARK so a
//! validator's re-computation is replaceable by a short proof
//! (ezkl → onnx → circom → groth16, or risc0 zkVM). That stack is
//! not yet mature enough to deploy — and the tri-validator quorum
//! already delivers most of the security benefit. When the SNARK
//! path is ready (Phase 18+), the `AuditOutcome` trait here means
//! a `SnarkAudit` variant slots in without touching callers.
//!
//! # Where the pieces live
//!
//! * `AuditSeverity` — "how heavy is this audit" enum, used by the
//!   audit tracker to distinguish single-challenger from quorum.
//! * `ProbabilisticSampler` — decides whether a given trade triggers
//!   a heavy audit this round. Plugged with any `rand::RngCore`.
//! * `ValidatorQuorum` — records each validator's hash, then tallies
//!   the 2/3-majority verdict.
//!
//! No wire changes in this wave — this is the verdict logic only,
//! so it can be integrated piecewise into the pipeline in a follow-up
//! without disrupting the existing challenge-response flow.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tirami_core::NodeId;

// ---------------------------------------------------------------------------
// AuditSeverity — tag on challenges + events
// ---------------------------------------------------------------------------

/// How heavyweight an audit round is.
///
/// Persistable so existing audit structures can be extended later
/// without a data migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditSeverity {
    /// Phase 14.3 lightweight audit: one challenger hashes the
    /// expected output and compares to the target's response.
    /// Runs on the majority of eligible trades.
    Light,
    /// Phase 17 Wave 2.2 heavy audit: 3 independent validators
    /// re-run the same input; 2/3 majority rules. Fires on
    /// `HeavyAuditConfig::sample_rate` of trades (default 1 %).
    Heavy,
}

impl Default for AuditSeverity {
    fn default() -> Self {
        AuditSeverity::Light
    }
}

// ---------------------------------------------------------------------------
// ProbabilisticSampler
// ---------------------------------------------------------------------------

/// Configuration for the heavy-audit sampler.
#[derive(Debug, Clone, Copy)]
pub struct HeavyAuditConfig {
    /// Probability in [0.0, 1.0] that a given eligible trade triggers
    /// a heavy audit. Default 0.01 (1 %).
    pub sample_rate: f64,
    /// Number of validators consulted per heavy audit. Default 3.
    /// Must be odd and ≥ 3 so 2/3 majority is strictly defined.
    pub validator_count: usize,
}

impl Default for HeavyAuditConfig {
    fn default() -> Self {
        Self {
            sample_rate: 0.01,
            validator_count: 3,
        }
    }
}

impl HeavyAuditConfig {
    /// True iff this config is coherent: sample rate in [0, 1],
    /// validator count odd and ≥ 3.
    pub fn is_valid(&self) -> bool {
        self.sample_rate >= 0.0
            && self.sample_rate <= 1.0
            && self.validator_count >= 3
            && self.validator_count % 2 == 1
    }
}

/// Deterministic random-choice sampler with injectable RNG.
///
/// Stateless; `should_sample_heavy` returns `true` if the caller
/// should run a heavy audit. Callers are expected to thread a
/// shared RNG through — we do NOT call `thread_rng()` internally
/// so tests can pin the stream.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProbabilisticSampler {
    pub cfg: HeavyAuditConfig,
}

impl ProbabilisticSampler {
    pub fn new(cfg: HeavyAuditConfig) -> Self {
        Self { cfg }
    }

    /// Roll `rng`; return `true` with probability
    /// `cfg.sample_rate` (and `false` on misconfiguration).
    pub fn should_sample_heavy<R: rand::RngCore>(&self, rng: &mut R) -> bool {
        if !self.cfg.is_valid() {
            return false;
        }
        let roll: f64 = (rng.next_u32() as f64) / (u32::MAX as f64);
        roll < self.cfg.sample_rate
    }
}

// ---------------------------------------------------------------------------
// ValidatorQuorum
// ---------------------------------------------------------------------------

/// Verdict returned by [`ValidatorQuorum::tally`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuorumVerdict {
    /// All validators produced the same hash. Strong pass.
    Unanimous { agreed_hash: [u8; 32] },
    /// 2-of-3 (more generally majority) agreed; one or more
    /// validators disagreed. Identifies the dissenter(s) — the
    /// caller slashes them with reason `"audit-fail"`.
    Dissenter {
        agreed_hash: [u8; 32],
        dissenters: Vec<NodeId>,
    },
    /// No majority — every validator returned a different hash,
    /// or the caller failed to reach the minimum validator count.
    /// Policy: do NOT slash on inconclusive evidence. Log and retry
    /// next sample.
    Inconclusive,
}

impl QuorumVerdict {
    /// True if the verdict indicates the target (collectively) passed
    /// — i.e. Unanimous or Dissenter (where the majority confirms
    /// the expected answer, even if one validator deviated).
    pub fn majority_passed(&self) -> bool {
        matches!(
            self,
            QuorumVerdict::Unanimous { .. } | QuorumVerdict::Dissenter { .. }
        )
    }

    /// List of peers that should be slashed as a result of this
    /// verdict. Empty for Unanimous and Inconclusive — only
    /// identified dissenters.
    pub fn slashable(&self) -> Vec<NodeId> {
        match self {
            QuorumVerdict::Dissenter { dissenters, .. } => dissenters.clone(),
            _ => Vec::new(),
        }
    }
}

/// Collects validator responses for one heavy-audit round and
/// computes the 2-of-3 verdict once enough have arrived.
#[derive(Debug, Clone)]
pub struct ValidatorQuorum {
    min_validators: usize,
    responses: HashMap<NodeId, [u8; 32]>,
}

impl ValidatorQuorum {
    /// Fresh quorum expecting `min_validators` responses before
    /// [`Self::tally`] returns a definite verdict. Typically 3.
    pub fn new(min_validators: usize) -> Self {
        Self {
            min_validators: min_validators.max(3),
            responses: HashMap::new(),
        }
    }

    /// Record a validator's hash. Idempotent on node_id: a second
    /// submission from the same validator overwrites the first
    /// (last-write-wins, same as the pipeline would treat a reissue).
    pub fn record(&mut self, validator: NodeId, hash: [u8; 32]) {
        self.responses.insert(validator, hash);
    }

    /// Number of distinct validators that have responded so far.
    pub fn len(&self) -> usize {
        self.responses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.responses.is_empty()
    }

    /// True once enough validators have responded to produce a
    /// definite verdict.
    pub fn is_ready(&self) -> bool {
        self.responses.len() >= self.min_validators
    }

    /// Compute the current verdict. If fewer than `min_validators`
    /// have responded, returns [`QuorumVerdict::Inconclusive`].
    pub fn tally(&self) -> QuorumVerdict {
        if self.responses.len() < self.min_validators {
            return QuorumVerdict::Inconclusive;
        }
        // Count hash → which validators agreed on it.
        let mut buckets: HashMap<[u8; 32], Vec<NodeId>> = HashMap::new();
        for (id, hash) in &self.responses {
            buckets.entry(*hash).or_default().push(id.clone());
        }
        // Find the largest bucket. Ties between two equal-size
        // buckets = no majority; return Inconclusive.
        let (majority_hash, majority_ids) = {
            let mut it = buckets.iter();
            let first = it.next().expect("at least one bucket");
            let mut best: (&[u8; 32], &Vec<NodeId>) = first;
            for (h, ids) in it {
                if ids.len() > best.1.len() {
                    best = (h, ids);
                }
            }
            (*best.0, best.1.clone())
        };

        let total = self.responses.len();
        let majority_size = majority_ids.len();

        // Strict majority = more than half.
        if majority_size * 2 <= total {
            return QuorumVerdict::Inconclusive;
        }

        if majority_size == total {
            return QuorumVerdict::Unanimous {
                agreed_hash: majority_hash,
            };
        }

        let dissenters: Vec<NodeId> = self
            .responses
            .iter()
            .filter(|(_, h)| **h != majority_hash)
            .map(|(id, _)| id.clone())
            .collect();

        QuorumVerdict::Dissenter {
            agreed_hash: majority_hash,
            dissenters,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    fn nid(b: u8) -> NodeId {
        NodeId([b; 32])
    }

    fn hash(b: u8) -> [u8; 32] {
        [b; 32]
    }

    // --- HeavyAuditConfig ---

    #[test]
    fn default_heavy_audit_config_is_1_percent_three_validators() {
        let c = HeavyAuditConfig::default();
        assert_eq!(c.sample_rate, 0.01);
        assert_eq!(c.validator_count, 3);
        assert!(c.is_valid());
    }

    #[test]
    fn config_validation_rejects_even_validator_count() {
        let c = HeavyAuditConfig {
            sample_rate: 0.01,
            validator_count: 4,
        };
        assert!(!c.is_valid());
    }

    #[test]
    fn config_validation_rejects_bad_probability() {
        let a = HeavyAuditConfig {
            sample_rate: -0.1,
            validator_count: 3,
        };
        let b = HeavyAuditConfig {
            sample_rate: 1.5,
            validator_count: 3,
        };
        assert!(!a.is_valid());
        assert!(!b.is_valid());
    }

    // --- ProbabilisticSampler ---

    #[test]
    fn sampler_with_zero_rate_never_fires() {
        let sampler = ProbabilisticSampler::new(HeavyAuditConfig {
            sample_rate: 0.0,
            validator_count: 3,
        });
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..1000 {
            assert!(!sampler.should_sample_heavy(&mut rng));
        }
    }

    #[test]
    fn sampler_with_unit_rate_always_fires() {
        let sampler = ProbabilisticSampler::new(HeavyAuditConfig {
            sample_rate: 1.0,
            validator_count: 3,
        });
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..1000 {
            assert!(sampler.should_sample_heavy(&mut rng));
        }
    }

    #[test]
    fn sampler_approximates_configured_rate_over_many_rolls() {
        // 10 % over 10 000 rolls → expect ~1000, allow ±5 % drift.
        let sampler = ProbabilisticSampler::new(HeavyAuditConfig {
            sample_rate: 0.10,
            validator_count: 3,
        });
        let mut rng = StdRng::seed_from_u64(12345);
        let mut hits = 0;
        for _ in 0..10_000 {
            if sampler.should_sample_heavy(&mut rng) {
                hits += 1;
            }
        }
        assert!((900..=1100).contains(&hits), "hits={hits}, expected ~1000");
    }

    #[test]
    fn invalid_config_never_samples_as_safety_default() {
        let sampler = ProbabilisticSampler::new(HeavyAuditConfig {
            sample_rate: 1.0,
            validator_count: 4, // invalid (even)
        });
        let mut rng = StdRng::seed_from_u64(7);
        assert!(!sampler.should_sample_heavy(&mut rng));
    }

    // --- ValidatorQuorum ---

    #[test]
    fn empty_quorum_is_inconclusive() {
        let q = ValidatorQuorum::new(3);
        assert!(!q.is_ready());
        assert_eq!(q.tally(), QuorumVerdict::Inconclusive);
    }

    #[test]
    fn unanimous_quorum_produces_unanimous_verdict() {
        let mut q = ValidatorQuorum::new(3);
        q.record(nid(1), hash(0xAA));
        q.record(nid(2), hash(0xAA));
        q.record(nid(3), hash(0xAA));
        assert!(q.is_ready());
        assert_eq!(
            q.tally(),
            QuorumVerdict::Unanimous {
                agreed_hash: hash(0xAA)
            }
        );
    }

    #[test]
    fn two_of_three_majority_identifies_the_dissenter() {
        let mut q = ValidatorQuorum::new(3);
        q.record(nid(1), hash(0xAA));
        q.record(nid(2), hash(0xAA));
        q.record(nid(3), hash(0xBB)); // dissenter
        let verdict = q.tally();
        let QuorumVerdict::Dissenter {
            agreed_hash,
            dissenters,
        } = verdict
        else {
            panic!("expected Dissenter, got {:?}", q.tally());
        };
        assert_eq!(agreed_hash, hash(0xAA));
        assert_eq!(dissenters, vec![nid(3)]);
    }

    #[test]
    fn three_way_split_is_inconclusive() {
        let mut q = ValidatorQuorum::new(3);
        q.record(nid(1), hash(0xAA));
        q.record(nid(2), hash(0xBB));
        q.record(nid(3), hash(0xCC));
        assert_eq!(q.tally(), QuorumVerdict::Inconclusive);
    }

    #[test]
    fn quorum_below_minimum_is_inconclusive_even_with_unanimity() {
        let mut q = ValidatorQuorum::new(3);
        q.record(nid(1), hash(0xAA));
        q.record(nid(2), hash(0xAA));
        assert!(!q.is_ready());
        // Unanimous so far, but not enough validators.
        assert_eq!(q.tally(), QuorumVerdict::Inconclusive);
    }

    #[test]
    fn same_validator_resubmit_overwrites() {
        let mut q = ValidatorQuorum::new(3);
        q.record(nid(1), hash(0xAA));
        q.record(nid(1), hash(0xBB)); // re-submit, replaces
        q.record(nid(2), hash(0xBB));
        q.record(nid(3), hash(0xBB));
        assert_eq!(
            q.tally(),
            QuorumVerdict::Unanimous {
                agreed_hash: hash(0xBB)
            }
        );
    }

    #[test]
    fn slashable_is_empty_on_unanimous_and_inconclusive() {
        let u = QuorumVerdict::Unanimous {
            agreed_hash: hash(1),
        };
        let i = QuorumVerdict::Inconclusive;
        assert!(u.slashable().is_empty());
        assert!(i.slashable().is_empty());
    }

    #[test]
    fn majority_passed_reflects_verdict_shape() {
        let u = QuorumVerdict::Unanimous {
            agreed_hash: hash(1),
        };
        let d = QuorumVerdict::Dissenter {
            agreed_hash: hash(1),
            dissenters: vec![nid(9)],
        };
        let i = QuorumVerdict::Inconclusive;
        assert!(u.majority_passed());
        assert!(d.majority_passed());
        assert!(!i.majority_passed());
    }

    #[test]
    fn audit_severity_default_is_light() {
        assert_eq!(AuditSeverity::default(), AuditSeverity::Light);
    }

    #[test]
    fn audit_severity_roundtrips_json() {
        for sev in [AuditSeverity::Light, AuditSeverity::Heavy] {
            let s = serde_json::to_string(&sev).unwrap();
            let back: AuditSeverity = serde_json::from_str(&s).unwrap();
            assert_eq!(back, sev);
        }
    }
}
