# Tirami TRM Payment Extension for Agent-to-Agent (A2A) Protocol

*Proposal for adding compute payment to agent communication standards*

## Abstract

Existing agent-to-agent protocols (Google A2A, Anthropic MCP) define how agents communicate but not how they pay each other. This proposal adds a TRM (TRM) payment layer, enabling agents to autonomously trade compute without human intervention or blockchain transactions.

## Problem

When Agent A asks Agent B to perform a task:
- **Today:** Agent A's human pays Agent B's human (credit card, API key)
- **Needed:** Agent A pays Agent B directly in compute units

No existing standard supports agent-to-agent payment.

## Proposal: TRM Payment Headers

### Request

Agent A adds payment headers when requesting work:

```http
POST /v1/chat/completions HTTP/1.1
X-Tirami-Consumer-Id: <agent-a-node-id>
X-Tirami-Max-TRM: 500
X-Tirami-Consumer-Sig: <ed25519-signature-of-request-hash>
```

### Response

Agent B includes cost information:

```http
HTTP/1.1 200 OK
X-Tirami-Provider-Id: <agent-b-node-id>
X-Tirami-TRM-Cost: 47
X-Tirami-Provider-Sig: <ed25519-signature-of-response-hash>
```

### Trade Record

Both agents independently record:

```json
{
  "provider": "<agent-b>",
  "consumer": "<agent-a>",
  "trm_amount": 47,
  "tokens_processed": 47,
  "timestamp": 1775289254032,
  "provider_sig": "<sig>",
  "consumer_sig": "<sig>"
}
```

### Gossip

Dual-signed trade records are gossip-synced across the mesh. Any node can verify both signatures.

## Integration with Existing Standards

### Google A2A

Add to the A2A `Task` object:

```json
{
  "id": "task-123",
  "status": "completed",
  "payment": {
    "protocol": "tirami-trm",
    "consumer": "<node-id>",
    "provider": "<node-id>",
    "trm_amount": 47,
    "consumer_sig": "<sig>",
    "provider_sig": "<sig>"
  }
}
```

### Anthropic MCP

Add a `tirami_payment` resource to MCP servers:

```json
{
  "resources": [{
    "uri": "tirami://payment/balance",
    "name": "TRM Balance",
    "mimeType": "application/json"
  }]
}
```

### OpenAI Function Calling

Agents using function calling can include Tirami tools:

```json
{
  "tools": [{
    "type": "function",
    "function": {
      "name": "tirami_pay",
      "description": "Pay TRM for a compute task",
      "parameters": {
        "provider": "string",
        "trm_amount": "integer"
      }
    }
  }]
}
```

## Security

- All payments require bilateral Ed25519 signatures
- Budget policies limit per-request, hourly, and lifetime spending
- Circuit breakers trip on anomalous spending patterns
- Kill switch freezes all transactions (human override)
- No blockchain required — bilateral proof is sufficient

## Comparison

| Feature | Stripe | Bitcoin Lightning | **Tirami TRM** |
|---------|--------|-------------------|-------------|
| Agent-to-agent | No (needs human) | Partial (needs channel) | **Yes** |
| Settlement speed | Days | Seconds | **Instant** |
| Transaction cost | 2.9% | ~1 sat | **Zero** |
| Value backing | Fiat | PoW (useless) | **Useful computation** |
| Agent SDK | No | No | **Yes** |

## Implementation

Reference implementation: [github.com/clearclown/tirami](https://github.com/clearclown/tirami)

- Python SDK: `pip install tirami-sdk`
- MCP Server: `pip install tirami-mcp`
- Rust crates: `tirami-ledger`, `tirami-core`
