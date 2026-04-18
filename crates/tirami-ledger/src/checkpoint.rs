//! Phase 17 Wave 2.4 — trade-log snapshotting + archival.
//!
//! # Problem
//!
//! `ComputeLedger::trade_log: Vec<TradeRecord>` grows unbounded.
//! At ~100 B/entry a seed node that processes 1 M trades sits on
//! ~100 MB of live heap, persisted back to `save_to_path` on every
//! flush. Multi-week operation bloats this further, and an
//! adversary can accelerate growth by hammering cheap trades.
//!
//! # Solution
//!
//! Periodically "seal" the trade log up to a cutoff timestamp:
//! 1. Compute a Merkle root over every trade `<= cutoff`.
//! 2. Append those trades to an on-disk archive (JSON-lines).
//! 3. Remove them from the in-memory `trade_log`.
//! 4. Record a [`LedgerCheckpoint`] proving which trades were sealed.
//!
//! After a seal:
//! * Historical lookups go through `replay_archive` (lazy-load the
//!   slice of archive lines whose timestamp range matches).
//! * The ledger's in-memory footprint is bounded by
//!   `now - cutoff` × trade rate, not the total lifetime.
//! * On-chain anchors (Phase 16 `tirami-anchor`) can reference the
//!   checkpoint's Merkle root for tamper evidence.
//!
//! # Wire / on-disk format
//!
//! Archive file: JSON-lines. One `TradeRecord` per line. Append-only.
//! A follow-up could compress to CBOR or snappy once archive files
//! cross GB; JSON-lines lets us debug with `less` for now.
//!
//! `LedgerCheckpoint` itself is Serde-friendly and rides on the normal
//! `PersistedLedger` snapshot so a restart doesn't forget the seals.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::TradeRecord;

// ---------------------------------------------------------------------------
// LedgerCheckpoint
// ---------------------------------------------------------------------------

/// Snapshot of the ledger's history up to (and including) a cutoff
/// timestamp. Emitted by [`seal_and_archive`] and persisted in
/// `ComputeLedger::checkpoints`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerCheckpoint {
    /// Merkle root of every trade with `timestamp <= sealed_up_to_ms`
    /// that existed in the ledger at seal time. Deterministic over
    /// `TradeRecord::canonical_bytes()`, so independent replayers
    /// can re-verify from the archive.
    pub merkle_root: [u8; 32],
    /// How many trades the seal folded into the archive this round.
    /// `sum(trade_count_sealed)` across checkpoints reconstructs the
    /// historical total.
    pub trade_count_sealed: u64,
    /// Upper bound (inclusive) on trade timestamps folded in.
    pub sealed_up_to_ms: u64,
    /// Wall-clock time the seal was performed (for dashboards).
    pub sealed_at_ms: u64,
}

impl LedgerCheckpoint {
    /// Empty/no-op checkpoint — no trades sealed. Useful when
    /// callers want a consistent return shape even on empty rounds.
    pub fn empty(sealed_at_ms: u64, sealed_up_to_ms: u64) -> Self {
        Self {
            merkle_root: [0u8; 32],
            trade_count_sealed: 0,
            sealed_up_to_ms,
            sealed_at_ms,
        }
    }

    /// True iff the checkpoint actually covers at least one trade.
    pub fn is_nonempty(&self) -> bool {
        self.trade_count_sealed > 0
    }
}

// ---------------------------------------------------------------------------
// Merkle root over canonical trade bytes
// ---------------------------------------------------------------------------

/// Compute a Merkle root over a slice of trades using their
/// `canonical_bytes()`. Pairs are combined left-to-right; the
/// last element in an odd layer is hashed with itself (Bitcoin
/// convention). Deterministic across Rust versions and platforms.
///
/// Returns `[0u8; 32]` for an empty slice so empty-seal checkpoints
/// have a well-defined root value.
pub fn trades_merkle_root(trades: &[TradeRecord]) -> [u8; 32] {
    if trades.is_empty() {
        return [0u8; 32];
    }
    let mut layer: Vec<[u8; 32]> = trades
        .iter()
        .map(|t| {
            let mut h = Sha256::new();
            h.update(t.canonical_bytes());
            h.finalize().into()
        })
        .collect();

    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len().div_ceil(2));
        let mut i = 0;
        while i < layer.len() {
            let left = layer[i];
            let right = if i + 1 < layer.len() {
                layer[i + 1]
            } else {
                layer[i] // Bitcoin-style duplication of the lone leaf.
            };
            let mut h = Sha256::new();
            h.update(left);
            h.update(right);
            next.push(h.finalize().into());
            i += 2;
        }
        layer = next;
    }
    layer[0]
}

// ---------------------------------------------------------------------------
// Archive I/O — JSON-lines, append-only
// ---------------------------------------------------------------------------

/// Errors surfaced by the archive I/O layer.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("archive encode: {0}")]
    Encode(String),
    #[error("archive decode: {0}")]
    Decode(String),
}

/// Append `trades` to `path` as JSON-lines. Creates parent dirs and
/// the file if needed. Flush is fsync'd so a crash mid-seal doesn't
/// leave half-written records that could be misread later.
pub fn append_archive(path: &Path, trades: &[TradeRecord]) -> Result<(), ArchiveError> {
    if trades.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut buf = String::new();
    for t in trades {
        let line = serde_json::to_string(t).map_err(|e| ArchiveError::Encode(e.to_string()))?;
        buf.push_str(&line);
        buf.push('\n');
    }
    file.write_all(buf.as_bytes())?;
    file.sync_data()?;
    Ok(())
}

/// Read all trades from a JSON-lines archive file. Intended for
/// historical queries — the live ledger does not call this on the
/// hot path.
pub fn read_archive(path: &Path) -> Result<Vec<TradeRecord>, ArchiveError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(ArchiveError::Io(e)),
    };
    let mut out = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let t: TradeRecord = serde_json::from_str(line)
            .map_err(|e| ArchiveError::Decode(format!("line {i}: {e}")))?;
        out.push(t);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Archive config helper
// ---------------------------------------------------------------------------

/// Convenience wrapper so callers can treat "no archive configured"
/// as a plain option rather than thread an Option everywhere.
#[derive(Debug, Clone, Default)]
pub struct ArchivePath(pub Option<PathBuf>);

impl ArchivePath {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self(Some(path.into()))
    }
    pub fn none() -> Self {
        Self(None)
    }
    pub fn as_path(&self) -> Option<&Path> {
        self.0.as_deref()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::NodeId;

    fn trade(ts: u64, amt: u64, seed: u8) -> TradeRecord {
        TradeRecord {
            provider: NodeId([seed; 32]),
            consumer: NodeId([seed.wrapping_add(1); 32]),
            trm_amount: amt,
            tokens_processed: amt / 10,
            timestamp: ts,
            model_id: "m".into(),
            flops_estimated: 0,
            nonce: {
                let mut n = [0u8; 16];
                n[0] = seed;
                n[15] = 0xAA; // ensure non-zero
                n
            },
        }
    }

    fn tmp_archive(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tirami-archive-{label}-{now}-{id}.jsonl"))
    }

    #[test]
    fn merkle_root_of_empty_slice_is_zero() {
        assert_eq!(trades_merkle_root(&[]), [0u8; 32]);
    }

    #[test]
    fn merkle_root_is_deterministic() {
        let ts = [trade(1, 10, 1), trade(2, 20, 2), trade(3, 30, 3)];
        let a = trades_merkle_root(&ts);
        let b = trades_merkle_root(&ts);
        assert_eq!(a, b);
    }

    #[test]
    fn merkle_root_changes_if_any_trade_changes() {
        let a = [trade(1, 10, 1), trade(2, 20, 2), trade(3, 30, 3)];
        let b = [trade(1, 10, 1), trade(2, 20, 2), trade(3, 31, 3)]; // amt changed
        assert_ne!(trades_merkle_root(&a), trades_merkle_root(&b));
    }

    #[test]
    fn merkle_root_odd_count_duplicates_last_leaf() {
        // Three trades — builds a 3-leaf tree where the lone leaf
        // on the right half is paired with itself. Verify against a
        // hand-computed reference to catch accidental reorderings.
        let ts = [trade(1, 10, 1), trade(2, 20, 2), trade(3, 30, 3)];
        let root = trades_merkle_root(&ts);

        let leaves: Vec<[u8; 32]> = ts
            .iter()
            .map(|t| {
                let mut h = Sha256::new();
                h.update(t.canonical_bytes());
                h.finalize().into()
            })
            .collect();
        let mut hi = Sha256::new();
        hi.update(leaves[2]);
        hi.update(leaves[2]); // dup self
        let right: [u8; 32] = hi.finalize().into();
        let mut lo = Sha256::new();
        lo.update(leaves[0]);
        lo.update(leaves[1]);
        let left: [u8; 32] = lo.finalize().into();
        let mut final_h = Sha256::new();
        final_h.update(left);
        final_h.update(right);
        let expected: [u8; 32] = final_h.finalize().into();
        assert_eq!(root, expected);
    }

    #[test]
    fn archive_append_and_read_roundtrips() {
        let path = tmp_archive("roundtrip");
        let ts = vec![trade(100, 1, 1), trade(200, 2, 2), trade(300, 3, 3)];
        append_archive(&path, &ts).unwrap();
        let back = read_archive(&path).unwrap();
        assert_eq!(back.len(), ts.len());
        for (a, b) in ts.iter().zip(back.iter()) {
            assert_eq!(a.provider, b.provider);
            assert_eq!(a.trm_amount, b.trm_amount);
            assert_eq!(a.nonce, b.nonce);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn archive_append_is_truly_append_only() {
        let path = tmp_archive("append");
        append_archive(&path, &[trade(1, 1, 1)]).unwrap();
        append_archive(&path, &[trade(2, 2, 2)]).unwrap();
        let back = read_archive(&path).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].timestamp, 1);
        assert_eq!(back[1].timestamp, 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn append_empty_slice_is_a_noop() {
        let path = tmp_archive("empty-append");
        assert!(append_archive(&path, &[]).is_ok());
        // File should NOT have been created for an empty append.
        assert!(!path.exists());
    }

    #[test]
    fn read_missing_archive_returns_empty() {
        let path = tmp_archive("missing");
        // File does not exist.
        assert!(!path.exists());
        let back = read_archive(&path).unwrap();
        assert!(back.is_empty());
    }

    #[test]
    fn read_archive_rejects_corrupt_line() {
        let path = tmp_archive("corrupt");
        std::fs::write(&path, "{not json\n").unwrap();
        let err = read_archive(&path).unwrap_err();
        assert!(matches!(err, ArchiveError::Decode(_)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ledger_checkpoint_empty_helper_is_consistent() {
        let c = LedgerCheckpoint::empty(1_000, 2_000);
        assert_eq!(c.merkle_root, [0u8; 32]);
        assert_eq!(c.trade_count_sealed, 0);
        assert_eq!(c.sealed_at_ms, 1_000);
        assert_eq!(c.sealed_up_to_ms, 2_000);
        assert!(!c.is_nonempty());
    }

    #[test]
    fn ledger_checkpoint_is_nonempty_tracks_count() {
        let mut c = LedgerCheckpoint::empty(0, 0);
        assert!(!c.is_nonempty());
        c.trade_count_sealed = 1;
        assert!(c.is_nonempty());
    }

    #[test]
    fn archive_path_wrapper_exposes_inner_path() {
        let with = ArchivePath::new("/tmp/foo.jsonl");
        assert!(with.as_path().is_some());
        let without = ArchivePath::none();
        assert!(without.as_path().is_none());
    }
}
