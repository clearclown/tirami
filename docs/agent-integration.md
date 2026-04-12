# Forge — Agent Integration Guide

## For AI Agent Developers

Forge gives your agent a compute budget. The agent can earn TRM by serving inference and spend TRM to access larger models. No credit card, no API key, no human in the loop.

## Quick Integration

### Any HTTP Client

```python
import requests

FORGE = "http://127.0.0.1:3000"

# Check if agent can afford a request
balance = requests.get(f"{FORGE}/v1/tirami/balance").json()
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
      "args": ["path/to/forge/mcp/tirami-mcp-server.py"]
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
curl localhost:3000/v1/tirami/balance

# Run inference
curl localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"hello"}]}'

# Check what it cost
curl localhost:3000/v1/tirami/trades
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
            print("Low TRM balance. Waiting to earn more...")
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
# Limit an agent to 1000 TRM per hour
curl -X POST localhost:3000/v1/tirami/policy \
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
curl localhost:3000/v1/tirami/safety
# Returns: hourly_spend, lifetime_spend, spends_last_minute
```

### Emergency Stop

```bash
# Freeze everything
curl -X POST localhost:3000/v1/tirami/kill \
  -d '{"activate": true, "reason": "agent anomaly"}'
```

## API Reference for Agents

| What agent needs | Endpoint | Method |
|-----------------|----------|--------|
| "How much TRM do I have?" | `/v1/tirami/balance` | GET |
| "How much will this cost?" | `/v1/tirami/pricing` | GET |
| "Who's the cheapest provider?" | `/v1/tirami/providers` | GET |
| "Run inference" | `/v1/chat/completions` | POST |
| "What did I spend?" | `/v1/tirami/trades` | GET |
| "Am I safe?" | `/v1/tirami/safety` | GET |
| "Cash out to Bitcoin" | `/v1/tirami/invoice` | POST |
| "STOP EVERYTHING" | `/v1/tirami/kill` | POST |

## Agent Borrowing Workflow

When an agent's TRM balance is insufficient for a task, it can borrow:

```python
from forge_sdk import ForgeClient

forge = ForgeClient()

def agent_with_borrowing():
    balance = forge.balance()
    pricing = forge.pricing()
    
    task_cost = pricing["estimated_cost_1000_tokens"] * 2  # ~2K tokens needed
    
    if balance["effective_balance"] < task_cost:
        # Check credit score
        credit = forge.credit()
        if credit["score"] > 0.3:
            # Borrow the shortfall
            shortfall = task_cost - balance["effective_balance"]
            loan = forge.borrow(
                amount=shortfall,
                term_hours=4,
                collateral=shortfall // 3
            )
            print(f"Borrowed {loan['principal_cu']} TRM at {loan['interest_rate']}%/hr")
    
    # Execute the task
    result = forge.chat("Complex analysis task...", max_tokens=2000)
    print(f"Cost: {result['cu_cost']} CU")
    
    # Repay from earnings
    forge.repay(loan_id=loan["id"])
    print(f"Loan repaid. Credit score improving.")
```

## Credit Building Pattern

New agents start with a credit score of 0.3. To build credit:

```python
def build_credit(forge):
    """Gradually build credit through reliable behavior."""
    
    # Phase 1: Earn through inference (builds trade history)
    # Serve inference normally -- every completed trade improves trade_score
    
    # Phase 2: Small borrow-repay cycles (builds repayment history)
    loan = forge.borrow(amount=100, term_hours=1, collateral=50)
    # ... do useful work ...
    forge.repay(loan_id=loan["id"])
    
    # Phase 3: Check progress
    credit = forge.credit()
    print(f"Credit score: {credit['score']}")
    print(f"  Trade score:     {credit['components']['trade']}")
    print(f"  Repayment score: {credit['components']['repayment']}")
    print(f"  Uptime score:    {credit['components']['uptime']}")
    print(f"  Age score:       {credit['components']['age']}")
    
    # Typical progression:
    # Week 1:  0.3 → 0.4 (initial trades + first repayment)
    # Month 1: 0.4 → 0.6 (consistent trades + multiple repayments)
    # Month 3: 0.6 → 0.8 (established history)
```

## API Reference for Lending

| What agent needs | Endpoint | Method |
|-----------------|----------|--------|
| "What's my credit score?" | `/v1/tirami/credit` | GET |
| "How much can I borrow?" | `/v1/tirami/pool` | GET |
| "Borrow CU" | `/v1/tirami/borrow` | POST |
| "Repay my loan" | `/v1/tirami/repay` | POST |
| "Lend my idle CU" | `/v1/tirami/lend` | POST |
| "View my loans" | `/v1/tirami/loans` | GET |

## Credit Score Factors

| Factor | Weight | How to improve |
|--------|--------|----------------|
| Trade history | 30% | Complete more inference trades (both as provider and consumer) |
| Repayment history | 40% | Repay loans on time — this is the largest factor |
| Uptime | 20% | Stay online and available for inference requests |
| Account age | 10% | Time on the network (maxes out at 90 days) |

**Note:** Credit scores are computed locally by each node from observed behavior. There is no central credit bureau. Different nodes may compute slightly different scores for the same peer based on their own observations.
