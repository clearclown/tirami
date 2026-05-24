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
TIRAMI_PORT="${TIRAMI_PORT:-3000}"
TIRAMI_AGENT_INTERVAL_SECONDS="${TIRAMI_AGENT_INTERVAL_SECONDS:-600}"
TIRAMI_AGENT_MAX_TOKENS="${TIRAMI_AGENT_MAX_TOKENS:-64}"
TIRAMI_AGENT_ESTIMATED_TRM="${TIRAMI_AGENT_ESTIMATED_TRM:-1}"
TIRAMI_LOG_DIR="${TIRAMI_LOG_DIR:-$HOME/.tirami/logs}"
TIRAMI_AGENT_LOG_FILE="${TIRAMI_AGENT_LOG_FILE:-$TIRAMI_LOG_DIR/tirami-agent-loop.log}"
TIRAMI_AGENT_PID_FILE="${TIRAMI_AGENT_PID_FILE:-$HOME/.tirami/tirami-agent-loop.pid}"
export TIRAMI_API_TOKEN

mkdir -p "$TIRAMI_LOG_DIR"

if [[ -f "$TIRAMI_AGENT_PID_FILE" ]]; then
  old_pid="$(cat "$TIRAMI_AGENT_PID_FILE" 2>/dev/null || true)"
  if [[ -n "$old_pid" ]] && kill -0 "$old_pid" 2>/dev/null; then
    kill -TERM "$old_pid" 2>/dev/null || true
  fi
fi

(
  while true; do
    ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    prompt="Tirami private-lab heartbeat at ${ts}. Reply with one compact operational observation."
    {
      echo
      echo "== $ts =="
      "$TIRAMI_BIN" agent \
        --url "http://${TIRAMI_BIND}:${TIRAMI_PORT}" \
        chat \
        --size remote \
        --max-tokens "$TIRAMI_AGENT_MAX_TOKENS" \
        --estimated-trm "$TIRAMI_AGENT_ESTIMATED_TRM" \
        "$prompt"
    } >>"$TIRAMI_AGENT_LOG_FILE" 2>&1 || true
    sleep "$TIRAMI_AGENT_INTERVAL_SECONDS"
  done
) &

pid="$!"
echo "$pid" >"$TIRAMI_AGENT_PID_FILE"
echo "started tirami agent loop pid=$pid bind=$TIRAMI_BIND:$TIRAMI_PORT interval=${TIRAMI_AGENT_INTERVAL_SECONDS}s"
