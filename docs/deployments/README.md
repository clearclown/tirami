# Tirami On-Chain Deployments

Phase 19 / Tier C–D. This directory records every on-chain deploy
of the Tirami contracts so operators, auditors, and the community
can independently verify what's live.

## Current status

| Network | Chain ID | TRM ERC-20 | TiramiBridge | Deployed at |
|---|---|---|---|---|
| Base Sepolia (testnet) | 84532 | — (not yet) | — | — |
| Base mainnet | 8453 | **audit-gated** | **audit-gated** | **not deployed** |

The Base mainnet deploy is blocked on:
1. External security audit (Phase 17 Wave 3.3) — not started.
2. Multi-sig configured for `Ownable::owner` transfer.
3. ≥30-day clean operation on Base Sepolia.
4. Active bug bounty for ≥30 days (SECURITY.md).

See `docs/release-readiness.md` for the full tiered rollout plan.

## Reproducing a deploy

All deploys must go through `repos/tirami-contracts/Makefile`:

```bash
# one-time environment
export DEPLOYER_ADDRESS=0x...     # the EOA that broadcasts the tx
export PRIVATE_KEY=0x...          # OR use `--ledger` for HW signer
export BASESCAN_KEY=...           # from https://basescan.org/myapikey

cd repos/tirami-contracts
make preflight                    # fail-fast env check
make test                         # 15/15 forge tests GREEN
make deploy-base-sepolia          # free testnet ETH
```

Record the resulting addresses and the broadcast artifact
(`broadcast/Deploy.s.sol/84532/run-latest.json`) as
`docs/deployments/base-sepolia-<YYYY-MM-DD>.md` using the template
below. The record is what lets any third party audit the
deployment path without reading the Foundry internals.

### Deployment record template

```markdown
# Base Sepolia deploy — YYYY-MM-DD

**Deployer:** 0x...
**Tx (TRM):** https://sepolia.basescan.org/tx/0x...
**Tx (Bridge):** https://sepolia.basescan.org/tx/0x...

**TRM ERC-20:** 0x...
**TiramiBridge:** 0x...

**Bytecode hash (TRM):** 0x... (from `forge inspect src/TRM.sol:TRM bytecode`)
**Bytecode hash (Bridge):** 0x...

**Source verified:** yes (BaseScan link)
**Constructor args:** DEPLOYER_ADDRESS = 0x...

**Follow-up:** owner transfer to <TEAM_OR_MULTISIG>.
```

## Mainnet deploy — gated

`make deploy-base-mainnet` is wired but refuses to run unless:

- `AUDIT_CLEARANCE=yes` environment variable is set (operator
  attestation that an external audit signed off).
- `MULTISIG_OWNER` environment variable contains the multi-sig
  address that will receive `Ownable::owner`.
- The operator types the phrase `i-accept-responsibility` at an
  interactive prompt.

Any operator who bypasses these gates (e.g. by patching the
Makefile) is solely responsible for the deployment. The protocol
maintainers explicitly decline any warranty on unaudited mainnet
contracts — see `SECURITY.md` and `LICENSE` (MIT).

## Secondary-market disclaimer

TRM is **compute accounting**, not a financial product. Anyone in
the world may — without the protocol maintainers' knowledge or
endorsement — create a secondary market, bridge the ERC-20 to
other chains, or speculate on its value. The maintainers:

- **do not** market TRM as an investment.
- **do not** receive, sell, or speculate on TRM on behalf of the
  project.
- **cannot control** third-party tokenization or resale after
  the OSS release (MIT license explicitly permits this).

If you are considering holding TRM as a store of value, you are
making that judgement yourself. The protocol works without any
external market — `1 TRM ↔ 10⁹ FLOP` is the definitional anchor,
and the compute is the value. Everything else is emergent.

See `SECURITY.md` ("Secondary markets & third-party tokenization")
for the full text of the disclaimer the repo commits to.
