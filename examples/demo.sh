#!/bin/bash
# Tirami Live Demo — run this after: cargo build --release
# Usage: ./examples/demo.sh
set -e

TIRAMI="http://127.0.0.1:3000"
BIN="target/release/tirami"

if [ ! -f "$BIN" ]; then
    echo "Build first: cargo build --release"
    exit 1
fi

echo "Starting Tirami node..."
$BIN node -m "qwen2.5:0.5b" --port 3000 --ledger /tmp/tirami-demo.json 2>/dev/null &
PID=$!
trap "kill $PID 2>/dev/null" EXIT

for i in $(seq 1 30); do
    LOADED=$(curl -s $TIRAMI/health 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('model_loaded',''))" 2>/dev/null)
    [ "$LOADED" = "True" ] && break
    sleep 1
done

echo ""
echo "=== TIRAMI: Computation is Currency ==="
echo ""

echo "Balance: $(curl -s $TIRAMI/v1/tirami/balance | python3 -c 'import sys,json; print(json.load(sys.stdin)["effective_balance"])') TRM"
echo ""

echo "Inference: 'Say hello in Japanese'"
RESULT=$(curl -s $TIRAMI/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Say hello in Japanese, one word"}],"max_tokens":8}')
echo "$RESULT" | python3 -c "
import sys,json; d=json.load(sys.stdin)
print(f'  Response: {d[\"choices\"][0][\"message\"][\"content\"]}')
print(f'  Cost: {d[\"x_tirami\"][\"trm_cost\"]} TRM')
" 2>/dev/null
echo ""

echo "Trades: $(curl -s $TIRAMI/v1/tirami/trades | python3 -c 'import sys,json; print(json.load(sys.stdin)["count"])') recorded"
echo "Merkle: $(curl -s $TIRAMI/v1/tirami/network | python3 -c 'import sys,json; print(json.load(sys.stdin)["merkle_root"][:24])')..."
echo ""
echo "Every watt produced intelligence. Every TRM is accountable."
