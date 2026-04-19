# Base Sepolia deploy — step-by-step

> Ops guide for deploying `TRM.sol` and `TiramiBridge.sol` to Base Sepolia
> (L2 testnet). The user executes this manually — it requires a funded
> wallet that the maintainers cannot delegate.
>
> **Mainnet deploy is audit-gated.** See the dedicated section at the
> bottom; the `Makefile` physically refuses `deploy-base-mainnet` without
> three independent confirmations.

## Prerequisites

1. **Foundry toolchain installed.** `forge --version` should work.
2. **Wallet private key** funded with ≥ 0.05 ETH on Base Sepolia. Faucets:
   - https://www.alchemy.com/faucets/base-sepolia
   - https://docs.base.org/docs/tools/network-faucets (Base Builder Faucet
     requires a Coinbase account).
3. **Base Sepolia RPC URL.** Free options:
   - `https://sepolia.base.org`
   - Alchemy / Infura with a registered project (higher rate limit).
4. **Basescan Sepolia API key** for contract verification. Register at
   https://basescan.org/myapikey.

## Environment setup

```bash
cd repos/tirami-contracts  # from tirami repo root
cat > .env <<'EOF'
BASE_SEPOLIA_RPC_URL=https://sepolia.base.org
DEPLOYER_PRIVATE_KEY=0xYOUR_FUNDED_KEY_HERE
BASESCAN_API_KEY=YOUR_BASESCAN_API_KEY
MULTISIG_OWNER=0xYOUR_MULTISIG_OR_EOA_HERE
EOF
chmod 600 .env
```

**Do not commit `.env`.** The repository `.gitignore` already covers it;
verify with `git check-ignore .env`.

## Dry-run first

```bash
# 1. Confirm tests still pass
make test  # expect: 15 passing

# 2. Simulate the deploy without broadcasting
source .env && \
forge script script/Deploy.s.sol \
  --rpc-url "$BASE_SEPOLIA_RPC_URL" \
  --private-key "$DEPLOYER_PRIVATE_KEY"
# expect: gas estimate printed, no on-chain state change
```

## Broadcast + verify

```bash
make deploy-base-sepolia
# internally runs:
#   forge script script/Deploy.s.sol \
#     --rpc-url $BASE_SEPOLIA_RPC_URL \
#     --private-key $DEPLOYER_PRIVATE_KEY \
#     --broadcast --verify \
#     --etherscan-api-key $BASESCAN_API_KEY
```

On success, `broadcast/Deploy.s.sol/84532/run-latest.json` will contain
the deployed addresses. `84532` is the Base Sepolia chain ID.

## Post-deploy

1. Copy the deployed addresses into
   `docs/deployments/base-sepolia.md` (create if absent):
   ```markdown
   # Base Sepolia deployments

   | Contract | Address | Deployed at | Verified? |
   |----------|---------|-------------|-----------|
   | TRM (ERC-20) | 0x... | YYYY-MM-DD tx 0x... | yes (basescan link) |
   | TiramiBridge | 0x... | YYYY-MM-DD tx 0x... | yes (basescan link) |
   ```
2. Update the tirami anchor client config (`crates/tirami-anchor/src/lib.rs`
   `BaseClient`) to point at the deployed addresses. The current default
   is `MockChainClient`.
3. Run a tirami node with `--anchor-chain base-sepolia` and confirm
   `GET /v1/tirami/anchors` returns real `tx_hash` values within
   `config.anchor_interval_secs`.
4. Let the node run for ≥ 30 days — this is the public-launch gate in
   `docs/release-readiness.md § Tier C`. Record incident-free uptime in
   `docs/deployments/base-sepolia-stability-log.md`.

## Mainnet is gated — do not attempt without audit clearance

`make deploy-base-mainnet` refuses to run without **all three** of:

- `AUDIT_CLEARANCE=yes` — set only after receiving a clean report from an
  external auditor (candidates: Trail of Bits, Zellic, Open Zeppelin,
  Least Authority).
- `MULTISIG_OWNER=<addr>` — the Gnosis Safe that will own the deployed
  contracts.
- An interactive prompt where the operator types
  `i-accept-responsibility` verbatim.

See the `deploy-base-mainnet` target in `Makefile` for the enforcement
code. The mainnet deploy state is tracked in
`docs/release-readiness.md § Tier D`; the maintainers' public stance is
that deploy is not authorised until all three conditions hold.
