//! Bitcoin OP_RETURN anchoring of the Forge trade Merkle root.
//!
//! Phase 10 P6 stub — full implementation lands in the Phase 10 agent batch.
//! This module will build `OP_RETURN` scripts carrying the 32-byte
//! `compute_trade_merkle_root` output, ready to be embedded in a Bitcoin
//! transaction by an external wallet (LDK or otherwise).
#![allow(dead_code)]
