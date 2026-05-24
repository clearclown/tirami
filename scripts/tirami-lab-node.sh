#!/usr/bin/env bash
set -euo pipefail

ENV_FILE="${TIRAMI_LAB_ENV:-$HOME/.tirami/tirami-lab.env}"
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  source "$ENV_FILE"
fi

: "${TIRAMI_BIND:?set TIRAMI_BIND to this node's reachable Tailscale IP}"
: "${TIRAMI_API_TOKEN:?set TIRAMI_API_TOKEN in $ENV_FILE}"

TIRAMI_BIN="${TIRAMI_BIN:-$HOME/tirami-testnet/target/release/tirami}"
TIRAMI_MODEL="${TIRAMI_MODEL:-qwen2.5:0.5b}"
TIRAMI_PORT="${TIRAMI_PORT:-3000}"
TIRAMI_P2P_BIND="${TIRAMI_P2P_BIND:-0.0.0.0:7700}"
TIRAMI_LOG_DIR="${TIRAMI_LOG_DIR:-$HOME/.tirami/logs}"
TIRAMI_PID_FILE="${TIRAMI_PID_FILE:-$HOME/.tirami/tirami-node.pid}"
TIRAMI_LOG_FILE="${TIRAMI_LOG_FILE:-$TIRAMI_LOG_DIR/tirami-node.log}"
RUST_LOG="${RUST_LOG:-info,swarm_discovery=error,iroh::socket::transports::relay=error,iroh::socket::remote_map::remote_state=error,iroh_relay=error,noq_udp=error}"
export TIRAMI_API_TOKEN RUST_LOG

mkdir -p "$TIRAMI_LOG_DIR"

stop_pid() {
  local pid="$1"
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill -INT "$pid" 2>/dev/null || true
    for _ in {1..20}; do
      kill -0 "$pid" 2>/dev/null || return 0
      sleep 1
    done
    kill -TERM "$pid" 2>/dev/null || true
  fi
}

if [[ -f "$TIRAMI_PID_FILE" ]]; then
  stop_pid "$(cat "$TIRAMI_PID_FILE" 2>/dev/null || true)"
fi

while IFS= read -r pid; do
  stop_pid "$pid"
done < <(pgrep -f "target/release/tirami start" || true)

args=(
  start
  --model "$TIRAMI_MODEL"
  --bind "$TIRAMI_BIND"
  --port "$TIRAMI_PORT"
  --p2p-bind "$TIRAMI_P2P_BIND"
)

if [[ -n "${TIRAMI_BOOTSTRAP_PEERS:-}" ]]; then
  IFS=',' read -ra peers <<< "$TIRAMI_BOOTSTRAP_PEERS"
  for peer in "${peers[@]}"; do
    peer="${peer//[[:space:]]/}"
    [[ -n "$peer" ]] && args+=(--bootstrap-peer "$peer")
  done
fi

nohup "$TIRAMI_BIN" "${args[@]}" >"$TIRAMI_LOG_FILE" 2>&1 &
pid="$!"
echo "$pid" >"$TIRAMI_PID_FILE"
echo "started tirami node pid=$pid bind=$TIRAMI_BIND:$TIRAMI_PORT"
