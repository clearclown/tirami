#!/bin/bash
# Tirami — Run the short live demo (for screenshots/recordings)
# For the full Phase 1-19 exerciser, use scripts/demo-e2e.sh instead.
set -e

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="$REPO_ROOT/target/release/tirami"
MODEL="qwen2.5:0.5b"
PORT=3000
LEDGER="/tmp/tirami-demo-ledger.json"

if [ ! -f "$BIN" ]; then
    echo "Build first: cargo build --release"
    exit 1
fi

kill $(pgrep tirami) 2>/dev/null || true
sleep 1
rm -f "$LEDGER"

echo "Starting Tirami node..."
$BIN node -m "$MODEL" --port $PORT --ledger "$LEDGER" 2>/dev/null &
FPID=$!
trap "kill $FPID 2>/dev/null" EXIT

until curl -sf http://127.0.0.1:$PORT/health > /dev/null 2>&1; do sleep 0.5; done
echo "Ready."
echo ""

echo "=== TIRAMI: Computation is Currency ==="
echo ""

echo "Balance:"
curl -s http://127.0.0.1:$PORT/v1/tirami/balance | python3 -m json.tool
echo ""

echo "Inference #1:"
curl -s http://127.0.0.1:$PORT/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Say hello in Japanese, one word"}],"max_tokens":8}' \
  | python3 -m json.tool
echo ""

echo "Inference #2:"
curl -s http://127.0.0.1:$PORT/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"What is 2+2? Just the number"}],"max_tokens":4}' \
  | python3 -m json.tool
echo ""

echo "Trades:"
curl -s http://127.0.0.1:$PORT/v1/tirami/trades | python3 -m json.tool
echo ""

echo "Network + Merkle Root:"
curl -s http://127.0.0.1:$PORT/v1/tirami/network | python3 -m json.tool
echo ""

echo "Pricing:"
curl -s http://127.0.0.1:$PORT/v1/tirami/pricing | python3 -m json.tool
echo ""

echo "=== Every watt produced intelligence. Every TRM is accountable. ==="
