# Tirami — Operator Guide

- [Hardware requirements](#hardware-requirements)
- [Install](#install)
- [Configure](#configure)
- [Start the node](#start-the-node)
- [Monitoring](#monitoring)
- [Backup and persistence](#backup-and-persistence)
- [Anchoring to Bitcoin](#anchoring-to-bitcoin)
- [Troubleshooting](#troubleshooting)
- [Security checklist](#security-checklist)

---

## Hardware requirements

**CPU**: x86_64 or aarch64. AVX2 recommended for x86_64; NEON is used automatically on ARM. The protocol runs on anything from a Raspberry Pi to a workstation, but inference throughput (and therefore TRM earnings) scales with compute capacity.

**GPU** (optional but recommended):
- Apple Silicon: Metal acceleration is **enabled by default** when building on macOS (`--features metal` is included in the default feature set). All inference layers run on-chip.
- NVIDIA: build with `--features cuda` (requires CUDA toolkit + libcublas). ROCm (`--features rocm`) works for AMD GPUs.
- Vulkan: `--features vulkan` for cross-vendor GPU acceleration.
- CPU-only: omit all GPU features. AVX512 is used automatically when available.

**Disk**:
- Model files (GGUF): SmolLM2-135M ≈ 100 MB, Qwen2.5-0.5B ≈ 491 MB, Qwen2.5-1.5B ≈ 1.1 GB, Qwen2.5-3B ≈ 2.0 GB, Qwen2.5-7B ≈ 4.7 GB.
- Ledger: stored as JSON at `tirami-ledger.json` by default. Grows approximately 1 KB per trade (one trade ≈ one inference request). A node processing 100 requests/day accumulates roughly 36 MB/year.
- L2/L3/L4 state: each state file (`bank_state.json`, `marketplace_state.json`, `mind_state.json`) is typically under 1 MB unless you have thousands of registered agents.

**RAM**: 2–4 GB for small models (< 3B params), 8+ GB for 7B models. Metal and CUDA offload most weights to GPU memory; CPU-only runs require system RAM to hold the full model.

**Network**: TCP port for the HTTP API (default 3000, configurable). QUIC port for P2P iroh transport (chosen by iroh at startup, typically ephemeral). If you run a public seed node, the QUIC port must be reachable from the internet or forwarded through a relay.

---

## Install

**From source (recommended)**:

```bash
git clone https://github.com/clearclown/forge
cd forge
cargo build --release
# Binary is at: ./target/release/forge
```

Cold build on Apple Silicon M-series: 2–3 minutes. Incremental rebuild after a change: 12–30 seconds. Rust edition 2024, resolver v2.

**Install to PATH**:

```bash
cargo install --path crates/tirami-cli
# Now available as: forge
```

**Docker**: tracked in the roadmap (Phase 11+); not yet published. Until then, build from source.

**Environment variables** (read by the SDK and demo scripts, not the core daemon directly — the daemon uses CLI flags):

| Variable | Purpose |
|---|---|
| `FORGE_URL` | Base URL of a running node (default `http://127.0.0.1:3000`) |
| `FORGE_API_TOKEN` | Bearer token for protected endpoints |
| `FORGE_MODELS_DIR` | Directory to search for local GGUF files (overrides default HF cache path) |
| `FORGE_BANK_STATE_PATH` | Path for tirami-bank (L2) state persistence |
| `FORGE_MARKETPLACE_STATE_PATH` | Path for tirami-agora (L4) marketplace state |
| `FORGE_MIND_STATE_PATH` | Path for tirami-mind (L3) agent snapshot |

---

## Configure

All configuration fields come from `crates/tirami-core/src/config.rs`. The daemon resolves them in order: CLI flags → config file → `Config::default()`.

| Field | Default | Impact |
|---|---|---|
| `api_port` | `3000` | Port the HTTP API binds to. Change with `--port`. |
| `api_bind_addr` | `"127.0.0.1"` | Bind address. Set to `0.0.0.0` to accept external connections (requires `--api-token`). |
| `api_bearer_token` | `None` | When set, all `/v1/tirami/*` and `/v1/chat/*` routes require `Authorization: Bearer <token>`. `/metrics` and `/health` are always unauthenticated. |
| `ledger_path` | `None` | Path to `tirami-ledger.json`. If `None`, ledger is in-memory only and lost on restart. Always set this in production. |
| `bank_state_path` | `None` | Path for L2 tirami-bank strategy/portfolio state. Survives restarts if set. |
| `marketplace_state_path` | `None` | Path for L4 tirami-agora marketplace snapshot. Survives restarts if set. |
| `mind_state_path` | `None` | Path for L3 tirami-mind agent snapshot. Survives restarts if set. |
| `settlement_window_hours` | `24` | Default time window for the `GET /settlement` export. `0` = manual only. |
| `max_memory_gb` | `4.0` | Soft cap on memory dedicated to inference. Does not OOM-kill — inference layer may exceed this under load. |
| `max_prompt_chars` | `8192` | Maximum prompt length accepted. Requests exceeding this are rejected with 400. |
| `max_generate_tokens` | `1024` | Hard cap on tokens generated per request. Maps to `max_tokens` in the OpenAI API. |
| `max_concurrent_remote_inference_requests` | `4` | Limits simultaneous P2P inference calls (seed mode only). |

---

## Start the node

### Single-node HTTP API (`tirami node`)

No P2P. Serves the full 5-layer API locally. Use for local development, as an OpenAI-compatible drop-in, or when you don't want to expose P2P ports.

```bash
./target/release/tirami node \
  --model qwen2.5:0.5b \
  --port 3000 \
  --api-token "change-me-in-production" \
  --ledger /var/lib/forge/ledger.json
```

On first start with a model shortname, the GGUF is downloaded from HuggingFace into the default cache (typically `~/.cache/huggingface/`). Subsequent starts load from cache.

### P2P seed node (`tirami seed`)

Holds a model, earns TRM by serving inference requests from worker nodes. Requires public reachability on the QUIC port (or a relay address configured via `--relay`).

```bash
./target/release/tirami seed \
  --model qwen2.5:1.5b \
  --port 3001 \
  --api-token "change-me-in-production" \
  --ledger /var/lib/forge/ledger.json
```

The public key printed at startup is what workers use to connect. Keep it stable (tied to the Ed25519 keypair stored on first launch).

### P2P worker node (`tirami worker`)

Connects to a seed, offloads inference, spends TRM from its own ledger to pay the seed.

```bash
./target/release/tirami worker \
  --seed <seed-public-key-hex>
```

Optional relay for NAT traversal:

```bash
./target/release/tirami worker \
  --seed <seed-public-key-hex> \
  --relay "https://relay.example.com"
```

A worker node starts with 1,000 TRM (welcome loan, 0% interest, 72-hour term per parameters.md §3). Repaying the welcome loan builds the initial credit score from 0.3 to 0.4 (parameters.md §3 `welcome_loan_credit_bonus`).

---

## Monitoring

Prometheus metrics are exported at `/metrics` with no authentication required. The scrape target is intentionally unauthenticated so it can be added to a standard Prometheus config without token management.

**11 metric series exported** (from `tirami_ledger::metrics::TiramiMetrics`):

| Metric | Type | Description |
|---|---|---|
| `tirami_cu_contributed_total` | Counter | Total TRM earned by this node across all trades |
| `tirami_cu_consumed_total` | Counter | Total TRM spent by this node |
| `tirami_reputation{node_id}` | Gauge | Current reputation score (0.0–1.0, default 0.5 per parameters.md §7) |
| `tirami_trade_count_total` | Counter | Total trades recorded on this node's ledger |
| `tirami_active_loan_count` | Gauge | Number of open loans (as lender or borrower) |
| `tirami_pool_total_trm` | Gauge | Total TRM in the lending pool |
| `tirami_pool_reserve_ratio` | Gauge | Current reserve ratio (must stay ≥ 30% per parameters.md §5) |
| `tirami_collusion_tight_cluster_score` | Gauge | Tight-cluster detection score for this node |
| `tirami_collusion_volume_spike_score` | Gauge | Volume-spike detection score |
| `tirami_collusion_round_robin_score` | Gauge | Round-robin (Tarjan SCC) detection score |
| `tirami_collusion_trust_penalty` | Gauge | Effective trust penalty subtracted from reputation |

Metrics that depend on trading activity (pool, loan counts, collusion scores) start at zero or their default and only update after the first trade. This is normal.

**Prometheus scrape config**:

```yaml
scrape_configs:
  - job_name: forge
    static_configs:
      - targets: ["127.0.0.1:3000"]
    metrics_path: /metrics
    scrape_interval: 15s
```

**Grafana dashboard sketch**: create four panels — (1) TRM flow over time: `rate(tirami_cu_contributed_total[5m])` vs `rate(tirami_cu_consumed_total[5m])`; (2) reputation gauge 0–1 with threshold line at 0.5; (3) lending pool health: `tirami_pool_reserve_ratio` with alert below 0.3; (4) collusion scores as a stacked bar, alert if `tirami_collusion_trust_penalty > 0.1`.

---

## Backup and persistence

**Ledger** (`tirami-ledger.json`): written on graceful shutdown (SIGTERM / Ctrl-C). The file is HMAC-SHA256 protected — any file-level modification will be detected on next load and the file will be rejected.

**On-demand backup** for L2/L3/L4 state:

```bash
curl -X POST http://localhost:3000/v1/tirami/admin/save-state \
  -H "Authorization: Bearer $TOKEN"
```

This triggers immediate persistence of `bank_state_path`, `marketplace_state_path`, and `mind_state_path` (whichever paths are configured).

**Recommended cron** (every 5 minutes):

```bash
# crontab -e
*/5 * * * * curl -s -X POST http://localhost:3000/v1/tirami/admin/save-state \
  -H "Authorization: Bearer $(cat /etc/forge/api_token)" >> /var/log/forge-backup.log 2>&1
```

Or with tirami-sdk:

```python
from forge_sdk import TiramiClient
import schedule, time

client = TiramiClient(base_url="http://localhost:3000", token=open("/etc/forge/api_token").read().strip())
schedule.every(5).minutes.do(client.save_state)
while True:
    schedule.run_pending()
    time.sleep(1)
```

**Snapshot restore**: automatic on startup. If `ledger_path` points to a valid (HMAC-intact) JSON file, the ledger resumes from that snapshot. Same for L2/L3/L4 state paths. No manual intervention needed.

**Off-host backup**: copy `tirami-ledger.json` and the three state files to a remote location. A simple approach:

```bash
*/30 * * * * rsync -a /var/lib/forge/*.json backup-host:/forge-backups/$(hostname)/
```

---

## Anchoring to Bitcoin

Every tirami node maintains a Merkle root of its trade log. This root can be published to Bitcoin as an OP_RETURN transaction for immutable audit — no one can later deny that a set of trades existed at a given block height.

**Get the anchor payload**:

```bash
curl "http://localhost:3000/v1/tirami/anchor?network=mainnet" \
  -H "Authorization: Bearer $TOKEN"
# Returns:
# {
#   "merkle_root_hex": "8edd724d...",
#   "script_hex": "6a2846524745...",
#   "network": "Mainnet",
#   "payload_len": 40
# }
```

The `script_hex` is a valid 40-byte Bitcoin OP_RETURN payload (`6a28 FRGE <version> <merkle_root>`). It is within Bitcoin's 80-byte OP_RETURN limit.

**Current status**: `tirami-lightning`'s LDK wallet is scaffolded but not yet wired to broadcast anchor transactions automatically (Phase 11 work). Until then, broadcast manually via your own Bitcoin node:

```bash
# Cron: write anchor weekly, broadcast manually
0 0 * * 0 curl -s "http://localhost:3000/v1/tirami/anchor?network=mainnet" \
  -H "Authorization: Bearer $(cat /etc/forge/api_token)" \
  > /tmp/forge-anchor-$(date +%Y%m%d).json

# Then broadcast via bitcoin-cli:
# bitcoin-cli sendrawtransaction <your-signed-tx-with-script_hex-as-output>
```

---

## Troubleshooting

**Model load fails**: verify the GGUF path is correct (use `--model /absolute/path/to/model.gguf`). Add `--verbose` to see llama.cpp initialization output. Check that the file is not truncated (compare SHA-256 against HuggingFace manifest). Check `llama-cpp-2 = "0.1"` model compatibility — most GGUF architectures are supported; see `docs/compatibility.md` for the full list.

**Port in use**: change `--port`. Run `lsof -i :3000` to identify the conflicting process.

**No Metal acceleration on Apple Silicon**: verify the build was done on macOS with `cargo build --release` (Metal is default ON). Check the startup log for `[INFO] Model loaded (llama.cpp)` — if you see layer counts like `31/31 layers on Metal`, acceleration is active. If you see `0 layers on Metal`, the build is missing the Metal feature — rebuild.

**High CPU on inference**: reduce `max_tokens` in requests. Switch to a smaller model tier (Small tier = 1 CU/token per parameters.md §2 vs Frontier = 20 CU/token). If on CPU-only, this is expected — GPU offload is the primary path to fast inference.

**Ledger corruption (HMAC-SHA256 fail)**: the ledger file was modified outside of Tirami, or the disk had a write error. Restore from the last known-good backup. If no backup exists, delete `tirami-ledger.json` and start fresh (balance resets to 0, welcome loan issued again). All trades before the corruption are unrecoverable from the local file.

**Reputation stuck at 0.5**: this is `DEFAULT_REPUTATION` (parameters.md §7) — the correct starting value for a new node. Reputation only moves after remote observations from peers are received and gossip-synced. Verify that P2P is working (`tirami status --url http://localhost:3000`) and that at least one other node has observed your trades. In single-node mode (`tirami node`), reputation stays at 0.5 indefinitely — that is expected.

**`/metrics` returns empty GaugeVec**: some metrics only populate after the first trade. Run a test inference request, then re-scrape.

---

## Security checklist

- Set `--api-token` to a long random string (32+ chars). Never commit it to source control.
- Rotate the API token if it appears in logs, process listings, or is shared accidentally. After rotation, restart the node.
- Expose the HTTP API over HTTPS via a reverse proxy (nginx or Caddy) when accepting traffic from outside localhost. Tirami does not handle TLS termination.
- Firewall the QUIC port to only allow connections from expected peers if you're running a private mesh. Public seed nodes must leave the QUIC port open.
- Back up `tirami-ledger.json` off-host. A stolen or corrupted ledger file means a lost TRM balance.
- Never run `--bind 0.0.0.0` without `--api-token`. The default `127.0.0.1` binding protects against accidental public exposure.

---

## DDoS mitigation — Phase 17 Wave 3.4

Tirami is designed for adversarial-network deployment. A public seed
node will be probed and occasionally flooded. Plan for it from day
one; don't bolt mitigations on during an incident.

### 1. Put a WAF or proxy in front

Tirami's HTTP API (`:3000` by default) **does not** implement
layer-7 DDoS defenses — no JS challenges, no behavioral rate
limiting, no geo-blocking. Put a mature edge in front:

- **Cloudflare (free tier)** — turn on "Under Attack" mode during
  incidents; normal mode is enough for baseline protection.
- **Caddy** with the `rate_limit` module — terminates TLS and caps
  per-IP requests. Configuration example:

  ```caddyfile
  api.your-tirami.example {
    rate_limit {
      zone api_per_ip {
        key {remote_ip}
        events 60
        window 1m
      }
    }
    reverse_proxy 127.0.0.1:3000
  }
  ```

- **nginx** with `limit_req_zone` — equivalent to Caddy's
  `rate_limit`, works well if you already run nginx.

### 2. Per-node connection cap

Tirami's iroh transport accepts an unbounded number of concurrent
QUIC peers by default. For a public node, cap this via the new
`Config::max_concurrent_connections` (default **1 000**, set `0` for
unlimited). Beyond the cap, new incoming connections are
immediately dropped — legitimate peers can retry; a flood attacker
cannot exhaust file descriptors.

```toml
# tirami.toml
max_concurrent_connections = 1000
```

Tune higher if you're running a dedicated seed node with
`ulimit -n` raised. Do not set `0` (unlimited) on any node reachable
from the public internet.

### 3. Per-ASN rate limiting (Phase 17 Wave 2.3)

Set `asn_rate_limit_enabled = true` once you've wired an IP→ASN
resolver into the transport (see Wave-2.3-part-2 tracking issue).
Default limits (5 000 msg/s per ASN) collapse the
cloud-massed-Sybil multiplier.

### 4. SYN flood handling

iroh's QUIC implementation internally rate-limits the initial
handshake on SYN-equivalent (Initial packet) floods. At the OS
level you can also:

- Linux: `sysctl -w net.ipv4.tcp_syncookies=1` (applies to iroh's
  UDP socket buffer pressure indirectly).
- Set `ulimit -n 65536` so fd exhaustion isn't your first failure
  mode.

### 5. Backpressure on economic endpoints

The existing `forge_rate_limiter` caps `/v1/tirami/*` at 30
requests per second per token. Do NOT disable this; raise the
ceiling instead if legitimate traffic hits the limit.

### 6. Monitoring

Scrape `/metrics` (no auth required — it's a Prometheus target).
Alerts to configure:

- `tirami_active_peer_connections` approaching
  `max_concurrent_connections` → scale or raise cap.
- `tirami_auth_failures_per_minute` > 10 sustained → possibly a
  credential-stuffing attempt; lock down the token.
- `tirami_rate_limit_drops` non-zero → not necessarily an attack,
  but warrants a look at traffic patterns.

### 7. Incident runbook

When you see sustained DDoS:

1. Enable Cloudflare "Under Attack" or equivalent.
2. Drop `max_concurrent_connections` to 200 for 10 minutes to let
   queues drain.
3. Check `/v1/tirami/slash-events` — are any peers being
   legitimately penalized during the event?
4. If a specific ASN is dominating the inbound, add it to a
   temporary `StaticAsnResolver` block list.
5. After 30 minutes of stable traffic, revert the temporary limits.

---

See also: [docs/agent-integration.md](agent-integration.md) for SDK integration, [forge-economics/docs/05-banking.md](https://github.com/clearclown/forge-economics) for lending theory, and run `forge --help` for the full CLI reference.
