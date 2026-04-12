//! Bitcoin OP_RETURN anchoring of the Forge trade Merkle root.
//!
//! Given a ledger's current `compute_trade_merkle_root()` output (32 bytes),
//! this module builds an OP_RETURN script and a `Transaction` skeleton ready
//! for signing by an external wallet. It does NOT broadcast — that's the
//! wallet's job. The anchor provides a free integrity witness: any change
//! to the historical trade log becomes detectable via Merkle root mismatch,
//! and the anchor itself is immutable once confirmed in a Bitcoin block.
//!
//! Layout of the OP_RETURN payload (40 bytes, fits well under the 80-byte
//! standard relay limit):
//!
//! ```text
//!     bytes 0..4:   magic "FRGE" (0x46 0x52 0x47 0x45)
//!     byte  4:      version = 0x01
//!     byte  5:      network flag (0x00=mainnet, 0x01=testnet, 0x02=signet, 0x03=regtest)
//!     bytes 6..8:   reserved (two zero bytes, for future use)
//!     bytes 8..40:  merkle root (32 bytes, big-endian hex of SHA-256)
//! ```
//!
//! Total = 40 bytes.

use bitcoin::blockdata::opcodes;
use bitcoin::blockdata::script::Builder;
use bitcoin::script::PushBytesBuf;
use bitcoin::{Amount, Network, ScriptBuf, Transaction, TxOut};
use bitcoin::absolute::LockTime;
use bitcoin::transaction::Version;
use serde::{Deserialize, Serialize};

/// Payload magic bytes (ASCII "FRGE").
pub const ANCHOR_MAGIC: [u8; 4] = *b"FRGE";

/// Current anchor payload version.
pub const ANCHOR_VERSION: u8 = 0x01;

/// Total payload length in bytes.
pub const ANCHOR_PAYLOAD_LEN: usize = 40;

/// Wrap a raw 32-byte merkle root into the Phase 10 anchor payload format.
///
/// Returns exactly 40 bytes: 4 magic + 1 version + 1 network + 2 reserved + 32 root.
pub fn build_anchor_payload(merkle_root: &[u8; 32], network: Network) -> [u8; ANCHOR_PAYLOAD_LEN] {
    let mut payload = [0u8; ANCHOR_PAYLOAD_LEN];
    payload[0..4].copy_from_slice(&ANCHOR_MAGIC);
    payload[4] = ANCHOR_VERSION;
    payload[5] = network_flag(network);
    // bytes 6..8 remain 0x00 0x00 (reserved)
    payload[8..40].copy_from_slice(merkle_root);
    payload
}

fn network_flag(net: Network) -> u8 {
    match net {
        Network::Bitcoin => 0x00,
        Network::Testnet => 0x01,
        Network::Signet => 0x02,
        Network::Regtest => 0x03,
        _ => 0xFF, // unknown/future
    }
}

/// Build a minimal OP_RETURN script carrying the anchor payload.
///
/// The resulting script is `OP_RETURN <40 bytes>` — this fits under the
/// standardness limit (80 bytes) and does not create a spendable UTXO.
pub fn build_anchor_script(merkle_root: &[u8; 32], network: Network) -> ScriptBuf {
    let payload = build_anchor_payload(merkle_root, network);
    let mut push_bytes = PushBytesBuf::new();
    push_bytes
        .extend_from_slice(&payload)
        .expect("40-byte payload is within push limit");
    Builder::new()
        .push_opcode(opcodes::all::OP_RETURN)
        .push_slice(push_bytes.as_push_bytes())
        .into_script()
}

/// Human-readable anchor metadata (for the HTTP endpoint response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorRequest {
    /// Hex-encoded Merkle root being anchored.
    pub merkle_root_hex: String,
    /// Hex-encoded OP_RETURN script bytes.
    pub script_hex: String,
    /// Hex-encoded payload bytes (40 bytes).
    pub payload_hex: String,
    /// Bitcoin network for this anchor.
    pub network: String,
    /// Payload length in bytes (always 40 for v0x01).
    pub payload_len: usize,
}

impl AnchorRequest {
    pub fn new(merkle_root: &[u8; 32], network: Network) -> Self {
        let payload = build_anchor_payload(merkle_root, network);
        let script = build_anchor_script(merkle_root, network);
        Self {
            merkle_root_hex: hex::encode(merkle_root),
            script_hex: hex::encode(script.as_bytes()),
            payload_hex: hex::encode(payload),
            network: format!("{:?}", network),
            payload_len: ANCHOR_PAYLOAD_LEN,
        }
    }
}

/// Build an unsigned Transaction skeleton with a single OP_RETURN output and
/// NO inputs. The caller (external wallet) is expected to add inputs and
/// optional change output before signing.
///
/// The wallet is responsible for:
/// - Selecting UTXOs to cover fee
/// - Adding TxIn entries
/// - Adding a change TxOut
/// - Signing
/// - Broadcasting
pub fn build_anchor_tx_skeleton(merkle_root: &[u8; 32], network: Network) -> Transaction {
    Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: Vec::new(),
        output: vec![TxOut {
            value: Amount::ZERO,
            script_pubkey: build_anchor_script(merkle_root, network),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_has_correct_length() {
        let root = [0u8; 32];
        let payload = build_anchor_payload(&root, Network::Bitcoin);
        assert_eq!(payload.len(), ANCHOR_PAYLOAD_LEN);
    }

    #[test]
    fn test_payload_magic_and_version() {
        let root = [42u8; 32];
        let payload = build_anchor_payload(&root, Network::Bitcoin);
        assert_eq!(&payload[0..4], &ANCHOR_MAGIC);
        assert_eq!(payload[4], ANCHOR_VERSION);
        assert_eq!(payload[5], 0x00); // mainnet
        assert_eq!(&payload[8..40], &[42u8; 32]);
    }

    #[test]
    fn test_payload_network_flags() {
        let root = [0u8; 32];
        assert_eq!(build_anchor_payload(&root, Network::Bitcoin)[5], 0x00);
        assert_eq!(build_anchor_payload(&root, Network::Testnet)[5], 0x01);
        assert_eq!(build_anchor_payload(&root, Network::Signet)[5], 0x02);
        assert_eq!(build_anchor_payload(&root, Network::Regtest)[5], 0x03);
    }

    #[test]
    fn test_script_has_op_return_prefix() {
        let root = [1u8; 32];
        let script = build_anchor_script(&root, Network::Bitcoin);
        let bytes = script.as_bytes();
        assert_eq!(bytes[0], 0x6a); // OP_RETURN
        // Next should be push length (40 = 0x28)
        assert_eq!(bytes[1], 0x28);
        assert_eq!(&bytes[2..42], &build_anchor_payload(&root, Network::Bitcoin));
        assert_eq!(bytes.len(), 42); // 1 opcode + 1 length + 40 payload
    }

    #[test]
    fn test_script_fits_standard_relay_limit() {
        let root = [0u8; 32];
        let script = build_anchor_script(&root, Network::Bitcoin);
        assert!(
            script.as_bytes().len() <= 80,
            "OP_RETURN must be <= 80 bytes to be standard"
        );
    }

    #[test]
    fn test_tx_skeleton_has_single_op_return_output() {
        let root = [7u8; 32];
        let tx = build_anchor_tx_skeleton(&root, Network::Bitcoin);
        assert_eq!(tx.input.len(), 0);
        assert_eq!(tx.output.len(), 1);
        assert_eq!(tx.output[0].value, Amount::ZERO);
        assert!(tx.output[0].script_pubkey.is_op_return());
    }

    #[test]
    fn test_anchor_request_round_trip_serde() {
        let root = [0xABu8; 32];
        let req = AnchorRequest::new(&root, Network::Testnet);
        let json = serde_json::to_string(&req).unwrap();
        let back: AnchorRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.merkle_root_hex, req.merkle_root_hex);
        assert_eq!(back.payload_len, 40);
    }

    #[test]
    fn test_payload_differs_across_networks() {
        let root = [0u8; 32];
        let main = build_anchor_payload(&root, Network::Bitcoin);
        let test = build_anchor_payload(&root, Network::Testnet);
        assert_ne!(main, test); // network byte differs
    }

    // =========================================================================
    // Security tests: Bitcoin OP_RETURN boundary enforcement
    // =========================================================================

    #[test]
    fn sec_anchor_payload_always_exactly_40_bytes() {
        // For every interesting Merkle root value, the payload must be exactly 40 bytes.
        for &root_byte in &[0x00u8, 0xFF, 0x42, 0xAB, 0x01, 0x7F, 0x80] {
            for net in [Network::Bitcoin, Network::Testnet, Network::Signet, Network::Regtest] {
                let payload = build_anchor_payload(&[root_byte; 32], net);
                assert_eq!(
                    payload.len(),
                    40,
                    "anchor payload for root_byte={root_byte:#04x} on {net:?} must be exactly 40 bytes, got {}",
                    payload.len()
                );
            }
        }
    }

    #[test]
    fn sec_anchor_script_never_exceeds_80_bytes() {
        // Standard relay limit is 80 bytes for OP_RETURN.
        for net in [Network::Bitcoin, Network::Testnet, Network::Signet, Network::Regtest] {
            let script = build_anchor_script(&[0xABu8; 32], net);
            assert!(
                script.as_bytes().len() <= 80,
                "OP_RETURN script for {net:?} must be <= 80 bytes (relay limit), got {}",
                script.as_bytes().len()
            );
        }
    }

    #[test]
    fn sec_anchor_payload_magic_never_corrupted() {
        // Regardless of Merkle root content, bytes 0..4 must always equal ANCHOR_MAGIC.
        for &root_byte in &[0x00u8, 0xFF, 0x01] {
            let payload = build_anchor_payload(&[root_byte; 32], Network::Bitcoin);
            assert_eq!(
                &payload[0..4],
                &ANCHOR_MAGIC,
                "magic bytes must always be 'FRGE' regardless of root content"
            );
        }
    }

    #[test]
    fn sec_anchor_payload_version_always_0x01() {
        // Version byte must always be ANCHOR_VERSION = 0x01.
        for net in [Network::Bitcoin, Network::Testnet] {
            let payload = build_anchor_payload(&[0x99u8; 32], net);
            assert_eq!(
                payload[4],
                ANCHOR_VERSION,
                "version byte must always be {ANCHOR_VERSION:#04x}"
            );
        }
    }

    #[test]
    fn sec_anchor_payload_merkle_root_occupies_bytes_8_to_40() {
        // The Merkle root must appear verbatim in bytes 8..40 of the payload.
        let root = [0xDEu8; 32];
        let payload = build_anchor_payload(&root, Network::Bitcoin);
        assert_eq!(
            &payload[8..40],
            &root,
            "Merkle root must be at bytes 8..40 of anchor payload"
        );
    }

    #[test]
    fn sec_anchor_script_opcode_is_op_return() {
        // The first byte of the OP_RETURN script must be 0x6A.
        for &root_byte in &[0x00u8, 0xFF, 0xAB] {
            let script = build_anchor_script(&[root_byte; 32], Network::Bitcoin);
            assert_eq!(
                script.as_bytes()[0],
                0x6A,
                "first script byte must be OP_RETURN (0x6A)"
            );
        }
    }

    #[test]
    fn sec_anchor_payload_reserved_bytes_are_zero() {
        // Bytes 6 and 7 are reserved and must be 0x00.
        for net in [Network::Bitcoin, Network::Testnet, Network::Regtest] {
            let payload = build_anchor_payload(&[0x55u8; 32], net);
            assert_eq!(payload[6], 0x00, "reserved byte 6 must be 0x00 for {net:?}");
            assert_eq!(payload[7], 0x00, "reserved byte 7 must be 0x00 for {net:?}");
        }
    }
}
