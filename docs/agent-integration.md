# Forge — Agent Integration Guide

## For AI Agent Developers

Forge gives your agent a compute budget. The agent can earn CU by serving inference and spend CU to access larger models. No credit card, no API key, no human in the loop.

## Quick Integration

### Any HTTP Client

```python
import requests

FORGE = "http://127.0.0.1:3000"

# Check if agent can afford a request
balance = requests.get(f"{FORGE}/v1/forge/balance").json()
if balance["effective_balance"] > 100:
    # Run inference (costs CU)
    r = requests.post(f"{FORGE}/v1/chat/completions", json={
        "messages": [{"role": "user", "content": "What is gravity?"}],
        "max_tokens": 256
    }).json()
    print(r["choices"][0]["message"]["content"])
    print(f"Cost: {r['x_forge']['cu_cost']} CU")
```

### Python SDK

```python
from forge_sdk import ForgeClient, ForgeAgent

# Simple client
forge = ForgeClient()
result = forge.chat("Explain quantum computing")
print(f"Answer: {result['content']}")
print(f"Cost: {result['cu_cost']} CU, Balance: {result['balance']} CU")

# Autonomous agent with budget management
agent = ForgeAgent(max_cu_per_task=500)
while agent.has_budget():
    result = agent.think("What should I do next?")
    if result is None:
        break  # budget exhausted
```

### MCP (Claude Code, Cursor)

Add to your MCP settings:
```json
{
  "mcpServers": {
    "forge": {
      "command": "python",
      "args": ["path/to/forge/mcp/forge-mcp-server.py"]
    }
  }
}
```

The AI assistant can then use tools like `forge_balance`, `forge_pricing`, `forge_inference`.

### LangChain

```python
from langchain_openai import ChatOpenAI

llm = ChatOpenAI(
    base_url="http://127.0.0.1:3000/v1",
    api_key="not-needed",
    model="qwen2.5-0.5b-instruct-q4_k_m"
)
response = llm.invoke("Hello")
# x_forge metadata available in response headers
```

### curl

```bash
# Check balance
curl localhost:3000/v1/forge/balance

# Run inference
curl localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"hello"}]}'

# Check what it cost
curl localhost:3000/v1/forge/trades
```

## Agent Economic Loop

The recommended pattern for an autonomous agent:

```python
from forge_sdk import ForgeClient

forge = ForgeClient()

def agent_loop():
    while True:
        # 1. Check budget
        balance = forge.balance()
        if balance["effective_balance"] < 50:
            print("Low CU balance. Waiting to earn more...")
            time.sleep(60)
            continue

        # 2. Check pricing
        pricing = forge.pricing()
        cost_per_100 = pricing["estimated_cost_100_tokens"]

        # 3. Decide if task is worth the cost
        if cost_per_100 > 500:
            print("Market price too high. Waiting...")
            time.sleep(30)
            continue

        # 4. Execute
        result = forge.chat("Analyze this data...", max_tokens=200)
        print(f"Done. Cost: {result['cu_cost']} CU")

        # 5. Check safety
        safety = forge.safety()
        if safety["circuit_tripped"]:
            print("Circuit breaker tripped. Pausing...")
            time.sleep(300)
```

## Safety for Agent Developers

### Set Budget Policies

```bash
# Limit an agent to 1000 CU per hour
curl -X POST localhost:3000/v1/forge/policy \
  -H "Content-Type: application/json" \
  -d '{
    "node_id": "<agent_node_id>",
    "max_cu_per_hour": 1000,
    "max_cu_per_request": 100,
    "human_approval_threshold": 500
  }'
```

### Monitor Spend Velocity

```bash
curl localhost:3000/v1/forge/safety
# Returns: hourly_spend, lifetime_spend, spends_last_minute
```

### Emergency Stop

```bash
# Freeze everything
curl -X POST localhost:3000/v1/forge/kill \
  -d '{"activate": true, "reason": "agent anomaly"}'
```

## API Reference for Agents

| What agent needs | Endpoint | Method |
|-----------------|----------|--------|
| "How much CU do I have?" | `/v1/forge/balance` | GET |
| "How much will this cost?" | `/v1/forge/pricing` | GET |
| "Who's the cheapest provider?" | `/v1/forge/providers` | GET |
| "Run inference" | `/v1/chat/completions` | POST |
| "What did I spend?" | `/v1/forge/trades` | GET |
| "Am I safe?" | `/v1/forge/safety` | GET |
| "Cash out to Bitcoin" | `/v1/forge/invoice` | POST |
| "STOP EVERYTHING" | `/v1/forge/kill` | POST |
