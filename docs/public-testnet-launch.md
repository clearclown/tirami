# Tirami Public Testnet Launch Runbook

**Status:** 2026-04-27. For open public testnet preparation with
virtual TRM only. This is not a mainnet or token-sale launch.

This runbook is the operational path for making Tirami easy for
worldwide computers to join while keeping the first public network
auditable and recoverable.

For a diagram-first Japanese public explanation of what the 2026-04-26
private lab did and did not prove, see
[`tirami-note-ja.md`](tirami-note-ja.md).

## Launch shape

The first public network should have three rings:

| Ring | Scope | Goal |
|---|---|---|
| Ring 0 | 2-3 maintainer bootstrap seeds | Stable public peer IDs, metrics, backup, incident response |
| Ring 1 | 10-30 invited operators | Multi-region smoke, NAT/relay coverage, memory and disk growth |
| Ring 2 | Open public testnet | Anyone can run `tirami start` with the published bootstrap peers |

TRM remains virtual during all three rings. Do not attach real
external value until the external audit, live bug bounty, and
30-day Sepolia gates in `docs/release-readiness.md` are closed.

## Ring 0 bootstrap seeds

Build from the release branch:

```bash
cargo build --release
cargo test --workspace
```

Create a dedicated host user and persistent state directory:

```bash
sudo useradd --system --create-home --home-dir /var/lib/tirami tirami
sudo install -d -o tirami -g tirami -m 700 /var/lib/tirami
sudo install -d -o tirami -g tirami -m 750 /var/log/tirami
```

Run the first bootstrap seed:

```bash
export TIRAMI_API_TOKEN="$(openssl rand -hex 32)"

./target/release/tirami start \
  --model qwen2.5:1.5b \
  --p2p-bind 0.0.0.0:7700 \
  --bind 0.0.0.0 \
  --port 3000
```

Record the startup log values:

- `Public key`
- `Node ID`
- `Full address`
- selected relay URL if present in the full address

Run the second and third bootstrap seeds with the first seed as a
bootstrap peer:

```bash
export TIRAMI_API_TOKEN="$(openssl rand -hex 32)"
export TIRAMI_BOOTSTRAP_PEERS="<seed-a-public-key>@https://relay.example.com"

./target/release/tirami start \
  --model qwen2.5:1.5b \
  --bind 0.0.0.0 \
  --port 3000
```

Publish at least two join strings. Example:

```text
<seed-a-public-key>@https://relay.example.com
<seed-b-public-key>@https://relay.example.com
```

For Linux hosts, use the bundled examples:

```bash
sudo install -d -m 755 /etc/tirami
sudo install -m 600 deploy/public-testnet.env.example /etc/tirami/public-testnet.env
sudo install -m 644 deploy/tirami-public-testnet.service /etc/systemd/system/tirami.service
sudo systemctl daemon-reload
sudo systemctl enable --now tirami
```

Edit `/etc/tirami/public-testnet.env` before starting the unit.

## Public join command

This is the command that should appear in the announcement:

```bash
cargo install --git https://github.com/clearclown/tirami tirami-cli

export TIRAMI_API_TOKEN="$(openssl rand -hex 32)"
export TIRAMI_BOOTSTRAP_PEERS="<seed-a-public-key>@https://relay.example.com,<seed-b-public-key>@https://relay.example.com"

tirami start \
  --model qwen2.5:0.5b \
  --p2p-bind 0.0.0.0:7700 \
  --bind 127.0.0.1 \
  --port 3000
```

Operators who intentionally expose the HTTP API must bind publicly
and keep the token:

```bash
tirami start \
  --model qwen2.5:1.5b \
  --p2p-bind 0.0.0.0:7700 \
  --bind 0.0.0.0 \
  --port 3000
```

The CLI refuses public or wildcard binds without `--api-token` or
`TIRAMI_API_TOKEN`.

## Single-person Tailscale lab

These four Tailscale addresses are enough for Ring 0/Ring 1 prep:

| Address | Suggested role |
|---|---|
| `100.83.54.6` | Current local PC / operator console |
| `100.112.10.128` | Mac Studio, primary seed/provider |
| `100.107.30.86` | ASUS ROG X13, cross-platform node |
| `100.82.83.122` | HP/Kali notebook, churn/low-power node |

Use a fixed P2P port on every machine:

```bash
export TIRAMI_API_TOKEN="$(openssl rand -hex 32)"

tirami start \
  --model qwen2.5:1.5b \
  --p2p-bind 0.0.0.0:7700 \
  --bind 127.0.0.1 \
  --port 3000
```

After each node starts, copy its `Public key` from the log and build
direct bootstrap join strings:

```text
<mac-studio-public-key>@100.112.10.128:7700
<rog-x13-public-key>@100.107.30.86:7700
<hp-kali-public-key>@100.82.83.122:7700
```

Then restart every non-primary node with the primary/secondary seeds:

```bash
export TIRAMI_BOOTSTRAP_PEERS="<mac-studio-public-key>@100.112.10.128:7700"

tirami start \
  --model qwen2.5:0.5b \
  --p2p-bind 0.0.0.0:7700 \
  --bind 127.0.0.1 \
  --port 3000
```

Keep HTTP bound to `127.0.0.1` for a pure P2P private lab. If you
want agent HTTP dispatch over Tailscale, bind HTTP to the machine's
`100.x` address and keep the same `TIRAMI_API_TOKEN` on the nodes in
that lab:

```bash
export TIRAMI_API_TOKEN="$(openssl rand -hex 24)"

# Primary provider
tirami start \
  --model qwen2.5:0.5b \
  --bind 100.112.10.128 \
  --port 3000 \
  --p2p-bind 0.0.0.0:7700

# Second node
tirami start \
  --model qwen2.5:0.5b \
  --bind 100.107.30.86 \
  --port 3000 \
  --p2p-bind 0.0.0.0:7700 \
  --bootstrap-peer <primary-public-key>@100.112.10.128:7700
```

After `/v1/tirami/peers` shows the provider's `http_endpoint`, the
consumer can let the agent auto-select a provider. The local bearer is
forwarded to the selected peer, so this works when the private testnet
uses a shared token:

```bash
tirami agent \
  --url http://100.107.30.86:3000 \
  --api-token "$TIRAMI_API_TOKEN" \
  chat "Give one concise sentence about useful compute." \
  --size remote \
  --estimated-trm 8 \
  -n 8
```

Expected smoke result:

- the response prints `remote (via <provider-node-id>)`;
- the provider's `/v1/tirami/agent/status` increments `earned_today_trm`;
- the consumer's `/v1/tirami/agent/status` increments `spent_today_trm`;
- both nodes' `/status` show the same provider/consumer trade after restart.

### 2026-04-26 lab result

The current private lab has verified this path with:

- provider: Mac Studio `100.112.10.128`;
- consumer: ASUS ROG X13 `100.107.30.86`;
- model: `qwen2.5:0.5b`;
- P2P bind: `0.0.0.0:7700`;
- HTTP bind: each machine's Tailscale `100.x` address;
- auth: shared `TIRAMI_API_TOKEN`.

The ASUS node submitted a remote PersonalAgent task without a `peer`
hint. It discovered the Mac Studio from `PriceSignal.http_endpoint`,
forwarded the bearer token, received the model output, and recorded the
same provider/consumer trade locally. After two remote jobs:

```text
Mac Studio /v1/tirami/agent/status: earned_today_trm = 18
ASUS       /v1/tirami/agent/status: spent_today_trm  = 18
Both       /status: total_trades = 2, total_contributed_cu = 18, total_consumed_cu = 18
```

Both nodes were restarted after the trade, and the ledgers restored the
same trade records from disk.

## Pre-announcement checklist

- [ ] `cargo test --workspace` green on the release commit.
- [ ] `cargo install --path crates/tirami-cli` succeeds on a clean machine.
- [ ] Two bootstrap seeds restart with the same public key after reboot.
- [ ] A third machine joins through `TIRAMI_BOOTSTRAP_PEERS`.
- [ ] `curl http://127.0.0.1:3000/health` returns healthy on all seeds.
- [ ] `GET /v1/tirami/peers` shows at least two peers after gossip.
- [ ] `POST /v1/tirami/admin/save-state` persists ledger, bank, agora,
  mind, personal agent, and trade archive state.
- [ ] Prometheus is scraping `/metrics` from every bootstrap seed.
- [ ] Off-host backup exists for `node.key`, `ledger.json`,
  `bank_state.json`, `marketplace_state.json`, `mind_state.json`,
  `personal_agent.json`, and `trades.jsonl`.
- [ ] SECURITY.md private advisory flow is tested by a maintainer.
- [ ] The public announcement says "open public testnet", "virtual TRM",
  and "not a token sale".

## Seven-day stability run

Before calling Ring 2 healthy, keep a 7-day board with these minimum
signals:

| Signal | Target |
|---|---|
| Bootstrap seed uptime | 99%+ over 7 days |
| Connected peers | 10+ unique nodes |
| Signed trades | At least one end-to-end trade per day |
| Memory growth | Bounded, no monotonic unbounded leak |
| Trade archive growth | Append-only, restarts replay cleanly |
| Auth failures | Visible in logs, no unauthenticated admin access |
| Bootstrap failures | Individual seed loss does not partition the network |

## Incident switches

If the public network is attacked or wedges:

1. Rotate `TIRAMI_API_TOKEN` on exposed HTTP APIs.
2. Restart bootstrap seeds one at a time, never all at once.
3. Drop `max_concurrent_connections` temporarily if peer floods
   threaten file descriptors.
4. Keep P2P open if possible; disable only the HTTP edge first.
5. Preserve logs and state files before any destructive cleanup.

## Announcement constraints

Use these exact constraints in public messaging:

- Tirami is MIT-licensed open-source software.
- TRM is compute accounting in the testnet, not an investment product.
- There is no ICO, pre-mine, airdrop, or promised return.
- Mainnet and real-value deployments are external-audit gated.
- The project is seeking node operators, security reviewers, and
  systems builders, not buyers.
