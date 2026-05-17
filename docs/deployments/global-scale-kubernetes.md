# Tirami — Global-scale Kubernetes deployment guide

Issue #116 C10. Last updated Phase 25 batch3.

This guide assembles the operationally hardened defaults landed in
Phase 25 (#117 → #123, plus this PR) into a deployable manifest set
and dashboard plan. It is intentionally conservative — every
parameter mentioned below is honoured by the binary today; we don't
document settings that don't exist yet.

## Prerequisites

- Kubernetes 1.27+ (the readiness probe semantics rely on
  per-component status that landed with this guide; older clusters
  work but won't surface component-level state in their UI).
- Persistent volume for `--ledger`. The ledger is a JSON snapshot;
  `RWO` is fine.
- Optional: Prometheus + Grafana for the metrics path. Prometheus
  scraping interval ≥15 s is plenty.

## Recommended `Config` (production)

```toml
# /etc/tirami/config.toml — operator overrides
api_port = 3000
api_bind_addr = "0.0.0.0"
# Phase 17 W1.5: scoped tokens preferred; this is the admin fallback.
api_bearer_token = "${TIRAMI_ADMIN_TOKEN}"

# Phase 25 A3 — protect /metrics on public-facing nodes.
metrics_require_bearer = true

# Phase 21 W2 — stake gate on. (Default.)
stake_gate_enabled = true

# Phase 24 W4 — proof policy starts permissive; governance ratchets
# it forward as the network matures.
proof_policy = "optional"
zkml_backend = "ed-attest"

# Phase 25 C2 — global chat in-flight cap. Tune for the worker's
# tokens/sec budget; 64 fits a single H100 node serving Llama-3-8B.
chat_concurrency_cap = 64

# Phase 25 C4 — gossip dedup horizon. Raise for high-TPS nodes.
gossip_max_seen = 500_000

# Phase 25 C9 — bound damage from a slashing false-positive cluster.
max_slashes_per_tick = 100

# Phase 16 / C6 — anchor cadence. 600 = 10 min.
anchor_interval_secs = 600

# Phase 17 W1.3 — slashing cadence.
slashing_interval_secs = 300

# Phase 17 W3.4 — ASN-keyed welcome-loan limiter (requires resolver).
asn_rate_limit_enabled = true
max_concurrent_connections = 5_000
```

## Probes

```yaml
# k8s pod-spec excerpt — Phase 25 A1
livenessProbe:
  httpGet:
    path: /healthz
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 10
  failureThreshold: 3   # ~30 s grace
readinessProbe:
  httpGet:
    path: /readyz
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 5
  failureThreshold: 2   # ~10 s grace
```

The readiness probe's JSON response includes a `components` map
(`ledger`, `engine`, `governance`, `staking_pool`). The Prometheus
exporter `tirami_process_uptime_secs` is the corresponding fleet-wide
metric for crash-loop detection.

## Resource limits

A node serving inference + protocol both:

```yaml
resources:
  requests:
    cpu: "2"
    memory: "16Gi"
  limits:
    cpu: "8"
    memory: "32Gi"
```

A protocol-only node (no `--model`, only ledger + governance):

```yaml
resources:
  requests:
    cpu: "500m"
    memory: "2Gi"
  limits:
    cpu: "2"
    memory: "8Gi"
```

## Grafana panel suggestions

Use the Prometheus exposition format (`/metrics`) and graph:

1. `tirami_process_uptime_secs` (per pod, line graph). A drop to 0
   means a restart. Alert on crash-loop: rate-of-restarts > 1/min.
2. `tirami_protocol_version` (per pod, instant value). Heterogeneous
   value across the fleet → version-skew during rollout.
3. `tirami_supply_factor` (instant gauge). Tracks TRM cap headroom.
4. `tirami_total_burned` (counter). Sudden spikes = slashing event;
   correlate with `tirami_slash_events_total` if exposed.
5. `tirami_total_staked` (gauge). Network-wide stake securing
   inference; trends down during attack waves.
6. `tirami_pool_reserve_ratio` (gauge). Lending pool health; drop
   below 0.3 = pre-cascade.
7. `tirami_active_proposals` (gauge). Open governance work.

## Suggested alert rules

```yaml
# alertmanager.yaml excerpt
groups:
- name: tirami
  rules:
  - alert: TiramiPodCrashLooping
    expr: |
      changes(tirami_process_started_at_secs[5m]) > 2
    for: 5m
    labels: { severity: page }
    annotations:
      summary: "tirami pod restarted >2 times in 5min"
  - alert: TiramiPoolReserveLow
    expr: tirami_pool_reserve_ratio < 0.3
    for: 10m
    labels: { severity: page }
    annotations:
      summary: "lending pool reserve below 30%"
  - alert: TiramiUnexpectedBurnSpike
    expr: |
      increase(tirami_total_burned[15m]) > 1_000_000
    for: 5m
    labels: { severity: page }
    annotations:
      summary: "≥1M TRM burned via slashing in 15min"
  - alert: TiramiReadinessFlapping
    expr: |
      rate(probe_success[5m]) < 0.9
    for: 10m
    labels: { severity: ticket }
    annotations:
      summary: "/readyz returning 503 too often"
```

## Bootstrap considerations

- DNS-based bootstrap relay rotation is a separate Phase 25 follow-up
  (issue #116 C1, still open). Until then, manage `bootstrap_peers`
  via a ConfigMap and roll the deployment to rotate.
- Run ≥3 seeds across distinct AZs so any one losing the model
  doesn't drop the network's inference capability to zero.
- For the welcome-loan ASN limiter, supply an IP→ASN resolver
  sidecar (issue #116 C1 covers DNS-based rotation but not the
  resolver — bring your own GeoIP feed).

## What this guide does NOT cover

- WAL for ledger writes (issue #116 C3, open). The JSON snapshot
  approach is still correct for crash recovery as long as
  `--ledger` lives on durable storage; high-TPS deployments will
  want the WAL once it lands.
- PQ hybrid signatures (issue #116 C5, open). ML-DSA scaffolded
  but not yet wired to the signing path.
- Wave 5.2.1+ guest ELF for risc0 prove side (issue #116 B series,
  open).
