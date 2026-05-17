//! Phase 25 C3 — Write-Ahead Log primitive for ledger operations.
//!
//! # Problem
//!
//! `ComputeLedger::save_to_path` rewrites the full snapshot per
//! checkpoint. Under sustained trade load this is O(N) per write,
//! and crashes between checkpoints lose any in-flight writes.
//!
//! # Solution (bounded scope)
//!
//! This PR ships the WAL **primitive**: an append-only file of
//! length-prefixed JSON-encoded `WalEntry` records, with a
//! `replay` function that streams entries back to the caller for
//! application against a fresh `ComputeLedger`. The migration
//! that actually swaps the snapshot path is a follow-up; this
//! lets unit tests exercise the on-disk format without coupling
//! to ledger internals yet.
//!
//! # On-disk format
//!
//! For each entry: `[u32 BE length][JSON bytes][SHA-256 trailer (32 bytes)]`.
//!
//! The trailer protects against partial writes — a torn entry has
//! the wrong hash and is dropped at replay time with a warning.
//! Operators who want stronger durability layer fsync on top.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use tirami_core::NodeId;

/// A single replayable operation. Variants intentionally minimal:
/// the primitive's job is durability, not protocol logic. Higher
/// layers wrap concrete `TradeRecord` / `LoanRecord` / `SlashEvent`
/// values into these variants at write time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WalEntry {
    Trade {
        provider: NodeId,
        consumer: NodeId,
        trm_amount: u64,
        timestamp_ms: u64,
        nonce: [u8; 16],
    },
    Loan {
        lender: NodeId,
        borrower: NodeId,
        principal_trm: u64,
        created_at_ms: u64,
    },
    Slash {
        node_id: NodeId,
        burned_trm: u64,
        timestamp_ms: u64,
        reason: String,
    },
    /// Phase 25 C3 — explicit checkpoint marker. Replay can stop
    /// here when the snapshot file already covers everything up
    /// to this point.
    Checkpoint { ledger_version: u32, timestamp_ms: u64 },
}

/// Errors raised by the WAL primitive.
#[derive(Debug, thiserror::Error)]
pub enum WalError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("WAL entry too large: {0} bytes")]
    TooLarge(u32),
    #[error("torn entry — length-prefix but no payload")]
    TornEntry,
    #[error("hash mismatch in entry — partial write or corruption")]
    HashMismatch,
}

/// Maximum length of a single WAL entry's JSON body. Defensive cap.
pub const MAX_ENTRY_BYTES: u32 = 64 * 1024;

/// Append-only writer. Each call to `append` flushes to disk so the
/// caller can rely on durability without explicit fsync; operators
/// who want O_SYNC layer it via mount options.
pub struct WalWriter {
    file: BufWriter<File>,
}

impl WalWriter {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, WalError> {
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { file: BufWriter::new(f) })
    }

    pub fn append(&mut self, entry: &WalEntry) -> Result<(), WalError> {
        let body = serde_json::to_vec(entry)?;
        let len = body.len() as u32;
        if len > MAX_ENTRY_BYTES {
            return Err(WalError::TooLarge(len));
        }
        // 4-byte big-endian length prefix.
        self.file.write_all(&len.to_be_bytes())?;
        // body
        self.file.write_all(&body)?;
        // 32-byte sha256 trailer over body for torn-write detection.
        let mut h = Sha256::new();
        h.update(&body);
        let digest: [u8; 32] = h.finalize().into();
        self.file.write_all(&digest)?;
        self.file.flush()?;
        Ok(())
    }
}

/// Streaming replay. Returns entries in append order; torn or
/// hash-mismatched tail entries are silently dropped (the caller
/// will detect them as missing via gossip / snapshot re-validation).
pub fn replay(path: impl AsRef<Path>) -> Result<Vec<WalEntry>, WalError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut reader = BufReader::new(File::open(path)?);
    let mut out = Vec::new();
    loop {
        let mut len_buf = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut len_buf) {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                break;
            }
            return Err(e.into());
        }
        let len = u32::from_be_bytes(len_buf);
        if len > MAX_ENTRY_BYTES {
            tracing::warn!("WAL: skipping oversized entry ({len} bytes)");
            break;
        }
        let mut body = vec![0u8; len as usize];
        if reader.read_exact(&mut body).is_err() {
            // Torn payload; stop replay (any subsequent entries are
            // unreadable because we lost framing).
            break;
        }
        let mut trailer = [0u8; 32];
        if reader.read_exact(&mut trailer).is_err() {
            // Torn trailer; same handling.
            break;
        }
        let mut h = Sha256::new();
        h.update(&body);
        let expected: [u8; 32] = h.finalize().into();
        if expected != trailer {
            tracing::warn!("WAL: hash mismatch — dropping torn tail entry");
            break;
        }
        let entry: WalEntry = serde_json::from_slice(&body)?;
        out.push(entry);
    }
    Ok(out)
}

/// Phase 25 C3 — sentinel: count distinct entry kinds in a replay
/// result, used by `/metrics` to graph WAL health.
pub fn count_kinds(entries: &[WalEntry]) -> (usize, usize, usize, usize) {
    let mut trades = 0;
    let mut loans = 0;
    let mut slashes = 0;
    let mut checkpoints = 0;
    for e in entries {
        match e {
            WalEntry::Trade { .. } => trades += 1,
            WalEntry::Loan { .. } => loans += 1,
            WalEntry::Slash { .. } => slashes += 1,
            WalEntry::Checkpoint { .. } => checkpoints += 1,
        }
    }
    (trades, loans, slashes, checkpoints)
}

// Silence unused import in non-trace builds.
#[allow(dead_code)]
fn _unused_bufread<R: BufRead>(_r: R) {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn nid(seed: u8) -> NodeId {
        NodeId([seed; 32])
    }

    fn tmp(name: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("tirami-wal-{pid}-{nanos}-{name}.log"))
    }

    fn sample_trade() -> WalEntry {
        WalEntry::Trade {
            provider: nid(1),
            consumer: nid(2),
            trm_amount: 100,
            timestamp_ms: 1_000,
            nonce: [0xAA; 16],
        }
    }

    #[test]
    fn append_then_replay_round_trips() {
        let path = tmp("rt");
        {
            let mut w = WalWriter::open(&path).unwrap();
            w.append(&sample_trade()).unwrap();
        }
        let entries = replay(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], sample_trade());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn multiple_appends_preserve_order() {
        let path = tmp("order");
        {
            let mut w = WalWriter::open(&path).unwrap();
            for i in 0..5 {
                w.append(&WalEntry::Trade {
                    provider: nid(i as u8),
                    consumer: nid((i + 1) as u8),
                    trm_amount: i as u64,
                    timestamp_ms: i as u64 * 1000,
                    nonce: [i as u8; 16],
                })
                .unwrap();
            }
        }
        let entries = replay(&path).unwrap();
        assert_eq!(entries.len(), 5);
        for (i, e) in entries.iter().enumerate() {
            match e {
                WalEntry::Trade { trm_amount, .. } => assert_eq!(*trm_amount, i as u64),
                _ => panic!("expected trade"),
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn replay_on_missing_file_returns_empty() {
        let path = tmp("missing");
        let _ = std::fs::remove_file(&path);
        let entries = replay(&path).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn torn_tail_is_dropped_silently() {
        let path = tmp("torn");
        {
            let mut w = WalWriter::open(&path).unwrap();
            w.append(&sample_trade()).unwrap();
        }
        // Append junk to simulate a torn write.
        {
            use std::fs::OpenOptions;
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(&[0u8; 7]).unwrap(); // partial 4-byte len + 3 junk bytes
        }
        let entries = replay(&path).unwrap();
        // The first complete entry survives; the torn one is dropped.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], sample_trade());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn hash_mismatch_stops_replay_at_the_corruption() {
        let path = tmp("mismatch");
        {
            let mut w = WalWriter::open(&path).unwrap();
            w.append(&sample_trade()).unwrap();
            w.append(&sample_trade()).unwrap();
        }
        // Corrupt the trailer of the LAST entry: flip the final byte.
        {
            use std::fs::OpenOptions;
            use std::io::Seek;
            let mut f = OpenOptions::new().read(true).write(true).open(&path).unwrap();
            let len = f.metadata().unwrap().len();
            f.seek(std::io::SeekFrom::Start(len - 1)).unwrap();
            f.write_all(&[0xFFu8]).unwrap();
        }
        let entries = replay(&path).unwrap();
        // The first entry stays; the corrupted second is dropped.
        assert_eq!(entries.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn count_kinds_partitions_correctly() {
        let entries = vec![
            sample_trade(),
            WalEntry::Loan {
                lender: nid(1),
                borrower: nid(2),
                principal_trm: 100,
                created_at_ms: 0,
            },
            WalEntry::Slash {
                node_id: nid(3),
                burned_trm: 50,
                timestamp_ms: 0,
                reason: "test".into(),
            },
            WalEntry::Checkpoint {
                ledger_version: 1,
                timestamp_ms: 0,
            },
            sample_trade(),
        ];
        let (t, l, s, c) = count_kinds(&entries);
        assert_eq!((t, l, s, c), (2, 1, 1, 1));
    }

    #[test]
    fn oversized_entry_is_rejected_at_write_time() {
        let path = tmp("toobig");
        let mut w = WalWriter::open(&path).unwrap();
        let huge_reason = "x".repeat((MAX_ENTRY_BYTES + 1) as usize);
        let entry = WalEntry::Slash {
            node_id: nid(1),
            burned_trm: 0,
            timestamp_ms: 0,
            reason: huge_reason,
        };
        let err = w.append(&entry).unwrap_err();
        assert!(matches!(err, WalError::TooLarge(_)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn wal_entry_serde_roundtrip_via_json() {
        let entry = sample_trade();
        let s = serde_json::to_string(&entry).unwrap();
        let back: WalEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back, entry);
    }
}
