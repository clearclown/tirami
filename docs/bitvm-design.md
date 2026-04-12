# BitVM Optimistic Verification for Forge TRM Claims

**Phase 12 A4 — Research Scaffold**

---

## 1. Motivation

Forge already anchors its trade history to Bitcoin. Phase 10 P6 (`tirami-ledger::anchor`)
embeds the trade-log Merkle root into an OP_RETURN output, producing a tamper-evident
commitment: if any node rewrites its past trade records, the new Merkle root will
diverge from what is permanently recorded on-chain.

That is powerful, but it is only half the story. OP_RETURN anchoring gives you *detection*
— an auditor who holds a copy of the old records can compare Merkle roots and notice
something changed. What it does **not** give you is *adjudication*: there is no on-chain
mechanism that lets a third party prove fraud to Bitcoin itself and collect a reward for
doing so. The attacker can always say "my ledger diverged because of a legitimate fork,
not because I cheated", and without a trusted arbitrator there is no way to settle the
dispute.

BitVM, introduced by Robin Linus in 2023, changes that picture. BitVM enables
off-chain optimistic execution with on-chain dispute resolution using only existing
Bitcoin opcodes (and, once activated, OP_CAT for more compact scripts). The key idea is
the *staked claim + challenge window* pattern:

1. A party stakes Bitcoin behind an assertion ("the Forge ledger at block N has Merkle
   root R").
2. Anyone observing a contradiction can post a fraud proof on-chain during the challenge
   window.
3. If the fraud proof is valid, Bitcoin Script enforces slashing automatically — no
   trusted arbitrator, no separate chain.

Combined with Forge's existing OP_RETURN anchors, this upgrades the system from
"tamper-evident" to "tamper-proven-and-slashable".

---

## 2. Threat Model

The following attacks are not fully addressed by plain OP_RETURN anchoring but are
addressed by BitVM-style staked claims:

**Post-hoc history rewrite (T10 extended).**
An attacker rewrites past trade records and re-anchors a new Merkle root. OP_RETURN
catches this only if an independent observer happens to compare roots and retained the
original records. BitVM lets *any* observer challenge the new root by posting a Merkle
inclusion proof for a trade that exists in the original anchor but is absent or different
in the re-anchored one. The staker's Bitcoin is at risk unless they can prove the
inclusion.

**Double-spend of CU.**
An attacker claims the same TRM balance in two forked ledger states — once to pay for
inference and once to repay a loan. The two different Merkle roots can both be anchored
with OP_RETURN, but neither anchor says anything about the other. A staked claim lets a
challenger post both root commitments and a conflicting-balance proof; the BitVM circuit
verifies the contradiction and enforces slashing.

**False signature claims.**
A trade record claims to be signed by NodeId X, but X's Ed25519 public key does not
verify the signature. Under OP_RETURN anchoring alone, detecting this requires trusting
the node that replays the gossip record. Under BitVM, a challenger can post the raw
trade bytes and the public key as evidence; Bitcoin Script (via a SHA256/hash opcode
sequence) can verify that the claimed signature is inconsistent.

**Free-rider TRM forgery (T10).**
A node fabricates TradeRecords crediting itself TRM it never earned. Gossip dedup and
dual signatures (Phases 4-5) largely close this, but not against a colluding
provider-consumer pair. A staked claim over the Merkle root means any honest peer who
observed the real trades can challenge a root that includes the forged ones.

---

## 3. High-Level Architecture

```
Forge ledger
    │
    ├─► anchor.rs (Phase 10 P6)
    │       builds OP_RETURN script with Merkle root
    │       → external wallet signs and broadcasts
    │
    └─► bitvm.rs (Phase 12 A4)
            StakedClaim: "ledger at block N has root R, I stake S sats"
            │
            ├─► FraudProof: posted by any challenger within the window
            │       fraud_type: MerkleInclusionMismatch | InvalidSignature
            │                   | DoubleSpend | InvalidBalanceUpdate
            │       evidence:   raw bytes for the Bitcoin Script verifier
            │
            └─► Settlement
                    if no valid FraudProof within challenge_window_blocks,
                    the claim is final and the staker recovers their stake
```

The integration point is `ComputeLedger`: when it calls `anchor::build_anchor_tx_skeleton`
to commit a Merkle root, it can also build a `StakedClaim` referencing the same root and
broadcast it (Phase 13 will add the actual Bitcoin signing and relay logic).

---

## 4. Primitives

### StakedClaim

An assertion that the Forge trade ledger is in a specific state at a specific Bitcoin
block height. Fields:

- `staker: NodeId` — who is putting up collateral
- `merkle_root: [u8; 32]` — the 32-byte SHA-256 Merkle root from `compute_trade_merkle_root()`
- `bitcoin_height: u64` — the block at which the claim is anchored
- `stake_sats: u64` — satoshis at risk (minimum 100,000 = 0.001 BTC)
- `challenge_window_blocks: u64` — default 2016 (~14 days at 10 min/block)
- `created_at_ms: u64` — wall-clock creation time

The minimum stake (100,000 sats) is chosen so that the cost of challenging (on-chain
fees) is always less than the reward, making honest challenging economically rational.

### FraudProof

A counter-example demonstrating inconsistency:

- `challenged_root: [u8; 32]` — must match the claim being challenged
- `challenger: NodeId` — who posted the proof (receives the slash reward)
- `fraud_type: FraudType` — category of the alleged inconsistency
- `evidence: Vec<u8>` — raw witness data (format is fraud_type-specific)

### FraudType

| Variant | What it proves |
|---------|---------------|
| `MerkleInclusionMismatch` | A Merkle inclusion proof for trade T contradicts the claimed root |
| `InvalidSignature` | A trade record's signature doesn't verify against the signer's public key |
| `DoubleSpend` | The same `trade_id` appears in two divergent history branches |
| `InvalidBalanceUpdate` | A balance delta violates TRM conservation across the claimed state |

### Challenge Window

Measured in Bitcoin blocks (not wall-clock time) to avoid timestamp manipulation.
Default: 2016 blocks ≈ 14 days. A claim becomes final once the window closes with no
valid challenges.

### Slashing

If `FraudProofVerifier::verify` returns `Ok(())`, the staker loses their stake. Phase 13
will implement the Bitcoin Script covenant that enforces this automatically. The split is
protocol-defined (e.g., 80% to challenger, 20% burned) to incentivise honest monitoring.

---

## 5. Why Not a Smart-Contract Chain?

Forge's core design rule is: **no blockchain in the critical path**. TRM accounting uses
local ledgers and gossip — 99%+ of trades never touch any external chain. Adding a
dependency on Ethereum or Solana would mean:

- Every Forge node needs an ETH/SOL wallet and gas budget.
- Network congestion on those chains directly impacts Forge's dispute resolution.
- Forge's economics become entangled with another chain's governance and token.

BitVM lets Forge use Bitcoin as a *neutral last-resort arbiter* only. Bitcoin is chosen
because it is the most credibly neutral settlement layer: no smart-contract governance,
no central foundation controlling opcodes, longest chain history. The cost of this choice
is circuit complexity (BitVM proofs are more verbose than Ethereum contracts), but for
Forge's use case — disputes happen rarely and the circuit is fixed at protocol design
time — this is an acceptable trade-off.

---

## 6. What This Phase 12 Scaffold Provides

This module is deliberately code-light and design-heavy. It provides:

- **`StakedClaim`** — complete type with constructor validation and `is_challengeable()`
- **`FraudProof`** + **`FraudType`** — complete types with serde support
- **`FraudProofVerifier` trait** — the interface Phase 13 will implement for real
- **`MockFraudProofVerifier`** — deterministic mock for unit tests; accepts a proof if
  the first evidence byte diverges from the claim's Merkle root
- **`BitVmError`** — structured error enum covering all failure modes
- **Re-exports** from `tirami_ledger` crate root so other crates can use the types
  without reaching into internals

The types are designed so Phase 13 can add real Bitcoin covenant logic by implementing
`FraudProofVerifier` without changing any of these interfaces.

---

## 7. What Is Out of Scope (Phase 13+)

The following are explicitly deferred to avoid speculative implementation:

- **BitVM circuit construction** — converting `FraudType` into a Bitcoin Script program
  that proves fraud in zero on-chain interaction steps. This requires secp256k1 tricks,
  OP_CAT (pending BIP 347 activation), and Taproot covenant patterns.
- **Real Bitcoin wallet integration** — posting the staked claim UTXO on-chain, watching
  the mempool for challenge transactions, and broadcasting slashing transactions.
- **Challenge-period monitoring** — a background task that watches for new `StakedClaim`
  commitments on Bitcoin and verifies each one against the local Forge ledger.
- **Watchtower relay services** — third-party nodes that monitor claims on behalf of
  light clients who cannot run a full Bitcoin node.
- **Stake recovery** — after the challenge window closes with no valid fraud proof,
  returning the staker's UTXO to a spendable output.

---

## 8. References

- Linus, Robin. "BitVM: Compute Anything on Bitcoin" (October 2023).
  https://bitvm.org/bitvm.pdf
- BIP 347: OP_CAT. https://github.com/bitcoin/bips/blob/master/bip-0347.mediawiki
- Forge economics §8-§9 — Bitcoin anchor discussion
  (`docs/economy.md`, `docs/monetary-theory.md`)
- `crates/tirami-ledger/src/anchor.rs` — Phase 10 P6 OP_RETURN anchor layer
- `docs/threat-model.md` — Economic threats T10-T17 that staked claims address
- Forge `compute_trade_merkle_root()` in `crates/tirami-ledger/src/ledger.rs`
