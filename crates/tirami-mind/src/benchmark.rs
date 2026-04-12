//! Benchmark abstraction.
//!
//! A `Benchmark` runs a harness against a fixed task set and returns a
//! `BenchmarkResult` (score in [0, 1]). Implementations vary:
//!
//! - `InMemoryBenchmark`: deterministic, fast, no inference cost. Used for tests.
//! - `LiveBenchmark` (planned): runs real inference via forge-sdk, costs CU.
//!
//! The benchmark must be **deterministic** for the same harness — running it
//! twice on the same harness must produce the same score. This is what makes
//! the improvement cycle reliable.

use crate::harness::Harness;
use crate::types::BenchmarkResult;

/// Abstract benchmark interface.
pub trait Benchmark: Send + Sync {
    fn name(&self) -> &str;
    fn evaluate(&self, harness: &Harness) -> BenchmarkResult;
}

/// A deterministic, in-memory benchmark.
///
/// Useful for tests and for the v0.1 scaffold. Caller supplies a scoring
/// function `f(harness) -> f64 in [0, 1]` and a sample count.
pub struct InMemoryBenchmark {
    name: String,
    scoring_fn: Box<dyn Fn(&Harness) -> f64 + Send + Sync>,
    sample_count: u32,
    trm_cost_per_run: u64,
}

impl InMemoryBenchmark {
    pub fn new(
        name: impl Into<String>,
        scoring_fn: impl Fn(&Harness) -> f64 + Send + Sync + 'static,
        sample_count: u32,
        trm_cost_per_run: u64,
    ) -> Self {
        Self {
            name: name.into(),
            scoring_fn: Box::new(scoring_fn),
            sample_count,
            trm_cost_per_run,
        }
    }

    /// Convenience constructor with default name and sample count.
    pub fn with_fn(scoring_fn: impl Fn(&Harness) -> f64 + Send + Sync + 'static) -> Self {
        Self::new("InMemoryBenchmark", scoring_fn, 100, 0)
    }
}

impl Benchmark for InMemoryBenchmark {
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(&self, harness: &Harness) -> BenchmarkResult {
        let score = (self.scoring_fn)(harness);
        // Clamp to [0, 1] for safety against scoring functions that return
        // marginally out-of-range values.
        let score = score.clamp(0.0, 1.0);
        BenchmarkResult::new(
            harness.version,
            score,
            self.sample_count,
            0,
            self.trm_cost_per_run,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Harness;

    #[test]
    fn test_in_memory_benchmark_returns_score() {
        let bench = InMemoryBenchmark::with_fn(|_| 0.7);
        let h = Harness::new("test".to_string());
        let result = bench.evaluate(&h);
        assert_eq!(result.score, 0.7);
        assert_eq!(result.harness_version, h.version);
    }

    #[test]
    fn test_benchmark_clamps_above_one() {
        let bench = InMemoryBenchmark::with_fn(|_| 1.5);
        let result = bench.evaluate(&Harness::new("x".to_string()));
        assert_eq!(result.score, 1.0);
    }

    #[test]
    fn test_benchmark_clamps_below_zero() {
        let bench = InMemoryBenchmark::with_fn(|_| -0.3);
        let result = bench.evaluate(&Harness::new("x".to_string()));
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn test_benchmark_records_trm_cost() {
        let bench = InMemoryBenchmark::new("test", |_| 0.5, 100, 42);
        let result = bench.evaluate(&Harness::new("x".to_string()));
        assert_eq!(result.cu_consumed, 42);
    }

    #[test]
    fn test_benchmark_is_deterministic() {
        let bench = InMemoryBenchmark::with_fn(|h| {
            if h.system_prompt.contains("be concise") {
                0.5
            } else {
                0.3
            }
        });
        let h = Harness::new("be concise".to_string());
        let a = bench.evaluate(&h);
        let b = bench.evaluate(&h);
        assert_eq!(a.score, b.score);
    }

    #[test]
    fn test_score_changes_with_harness() {
        let bench = InMemoryBenchmark::with_fn(|h| {
            0.5 + 0.05 * h.system_prompt.matches("good").count() as f64
        });
        let h1 = Harness::new("hello".to_string());
        let h2 = Harness::new("good good good".to_string());
        assert!(bench.evaluate(&h1).score < bench.evaluate(&h2).score);
    }
}
