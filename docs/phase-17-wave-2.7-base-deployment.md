# Phase 17 Wave 2.7 — Base Sepolia Deployment Runbook

Author: Phase 17 Wave 2 team · 2026-04-18

**Status:** Operator-action required. The code side is ready; actual
deployment needs a funded Sepolia wallet, an RPC endpoint, and a
30-day observation window before we consider it stable enough for
mainnet. Mainnet deploy remains **BLOCKED** until an external security
audit completes (see `docs/security/audit-scope.md`, Wave 3.3).

## Scope

This runbook covers:

1. Local `forge test` verification of `repos/tirami-contracts/`.
2. Deployment of `TRM.sol` + `TiramiBridge.sol` to the Base Sepolia
   testnet.
3. Wiring the deployed addresses into `tirami-node` via `Config`.
4. The production `BaseClient` implementation of
   `tirami_anchor::ChainClient` (scaffolded — full ethers-rs
   integration deferred to Wave 2.7-part-2 pending dependency-pin
   resolution).

Out of scope: Base mainnet deployment, bridge liquidity seeding,
multi-sig custody setup — all require the external audit to finish
first.

## Prerequisites

- Foundry ≥ 0.2.0 (`foundryup`)
- `forge` binary on PATH (`cargo install --locked forge` if missing)
- A Base Sepolia-funded EOA with ≥ 0.1 Sepolia ETH for deployment gas.
  Testnet faucet: <https://www.alchemy.com/faucets/base-sepolia>
- Base Sepolia RPC URL (Alchemy, Infura, or <https://sepolia.base.org>)
- The deployer private key stored in `$DEPLOYER_KEY` (do NOT commit).
- `$BASE_SEPOLIA_RPC_URL` env var pointing at your chosen RPC.

## Step 1 — Install OpenZeppelin

```bash
cd repos/tirami-contracts
forge install OpenZeppelin/openzeppelin-contracts --no-commit
```

The contracts import from `openzeppelin-contracts` for the ERC-20
(TRM) and access-control primitives. The `--no-commit` flag avoids
auto-committing the submodule pin, which we'll do once the install
is verified green.

## Step 2 — Run the full contract test suite locally

```bash
forge test -vv
```

Expected output:

```
Running 15 tests for test/TRM.t.sol:TRMTest
  [PASS] ... (all 15 passing)
Running ... test/TiramiBridge.t.sol:TiramiBridgeTest
  ...

Suite result: ok. 15+ passed; 0 failed; 0 skipped
```

If any test fails, STOP and escalate — every test must pass before
Sepolia deploy.

## Step 3 — Dry-run the deploy script

```bash
forge script script/Deploy.s.sol \
  --rpc-url $BASE_SEPOLIA_RPC_URL \
  --sender $DEPLOYER_ADDRESS \
  --private-key $DEPLOYER_KEY
```

This simulates the deploy against Sepolia state without broadcasting.
Confirm the gas estimate is sane (< 2 M gas for both contracts) and
the `console.log` output shows the expected deployment pattern.

## Step 4 — Broadcast the deploy

```bash
forge script script/Deploy.s.sol \
  --rpc-url $BASE_SEPOLIA_RPC_URL \
  --private-key $DEPLOYER_KEY \
  --broadcast \
  --verify \
  --etherscan-api-key $BASESCAN_KEY
```

On success you'll see:

```
TRM deployed at:    0x...
Bridge deployed at: 0x...
```

Record both addresses in:

- `docs/deployments/base-sepolia.md` (new file; template at the end
  of this runbook)
- A PR to `tirami-contracts/README.md`
- The `tirami-core/config.rs` default override for testnet operators.

## Step 5 — Verify on Basescan

Visit `https://sepolia.basescan.org/address/<TRM_ADDRESS>` and confirm:

- Contract source is verified (green checkmark).
- ERC-20 name = "Tirami Resource Merit", symbol = "TRM".
- Cap = `21_000_000_000 * 10^18` (21 B).
- Bridge address is set on the TRM contract.

Repeat for `TiramiBridge` — look for the event `BridgeConfigured`.

## Step 6 — Wire the address into tirami-node

The deployed bridge address becomes part of the anchor client's
configuration. The scaffolded `BaseClient` reads it from:

```rust
Config {
    chain_client_mode: ChainClientMode::BaseSepolia {
        bridge_address: "0x...".parse()?,
        rpc_url: "https://sepolia.base.org".to_string(),
    },
    ...
}
```

Until the real ethers-rs `BaseClient` ships (blocked on
`digest 0.11.0-rc.10` pin in iroh 0.97 — see the workspace
`Cargo.toml` comment), the node continues to use
`MockChainClient::default()`. The scaffold keeps the config variants
compile-clean so the switch is a single-line change once unblocked.

## Step 7 — 30-day stability watch

Before we consider the Sepolia deployment "stable enough to think
about mainnet":

- [ ] Run at least one `tirami-node` instance against the Sepolia
      deployment continuously for 30 calendar days.
- [ ] Verify every anchor batch lands on-chain via
      `GET /v1/tirami/anchors` cross-referenced with Basescan.
- [ ] Record gas costs per batch in
      `docs/deployments/base-sepolia-metrics.md`.
- [ ] Log any failed submissions (timeout, reorg, insufficient gas)
      and the root-cause analysis for each.
- [ ] At day 30, write a stability report concluding whether
      mainnet is a go.

## Mainnet deployment — BLOCKED

Mainnet deploy of TRM + TiramiBridge is gated on:

1. External security audit complete and all High/Critical findings
   resolved (Wave 3.3; candidates: Trail of Bits, OpenZeppelin,
   Zellic, Least Authority).
2. 30 days stable operation on Sepolia per Step 7.
3. Multi-sig custody configured and tested on Sepolia (not a raw EOA
   owner).
4. Bug bounty program live and accepting submissions for ≥ 30 days.

The current workspace `Cargo.toml` — and the `BaseClient` scaffold —
both include defensive comments to this effect. If any reviewer sees
a PR that flips the chain mode to Base mainnet before these four
gates are green, reject it.

## Deployment record template

File: `docs/deployments/base-sepolia.md` (create on first deploy)

```markdown
# Tirami Contracts on Base Sepolia

Deployed: <YYYY-MM-DD HH:MM UTC>
Deployer: <EOA address>
Block:    <block number>

## Contracts

| Contract    | Address                                     | Basescan |
|-------------|---------------------------------------------|----------|
| TRM         | 0x000000000000000000000000000000000000DEAD  | [link]   |
| TiramiBridge| 0x000000000000000000000000000000000000DEAD  | [link]   |

## Initial state

- TRM owner: <EOA>
- TRM bridge: <TiramiBridge address>
- Bridge owner: <EOA>

## Known issues

(none at deploy time)

## Migration plan

If the contracts need to be redeployed, follow
`docs/phase-17-wave-2.7-base-deployment.md` Step 7 recovery
procedure and pin the new addresses here.
```
