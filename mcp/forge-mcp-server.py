#!/usr/bin/env python3
"""
Forge MCP Server — lets AI agents interact with the Forge compute economy.

Install: pip install mcp httpx
Run:     python forge-mcp-server.py

Add to Claude Code settings:
{
  "mcpServers": {
    "forge": {
      "command": "python",
      "args": ["path/to/forge-mcp-server.py"]
    }
  }
}

The agent can then:
- Check its CU balance
- Get pricing before making decisions
- View trade history
- Query network economic state
- Create Lightning invoices
- Activate emergency kill switch
"""

import asyncio
import json
import os
import sys

try:
    import httpx
    from mcp.server import Server
    from mcp.server.stdio import stdio_server
    from mcp.types import Tool, TextContent
except ImportError:
    print("Install dependencies: pip install mcp httpx", file=sys.stderr)
    sys.exit(1)

FORGE_URL = os.environ.get("FORGE_URL", "http://127.0.0.1:3000")
FORGE_TOKEN = os.environ.get("FORGE_API_TOKEN", "")

server = Server("forge-economy")
client = httpx.AsyncClient(timeout=30.0)


def headers():
    h = {"Content-Type": "application/json"}
    if FORGE_TOKEN:
        h["Authorization"] = f"Bearer {FORGE_TOKEN}"
    return h


async def forge_get(path: str) -> dict:
    r = await client.get(f"{FORGE_URL}{path}", headers=headers())
    r.raise_for_status()
    return r.json()


async def forge_post(path: str, data: dict) -> dict:
    r = await client.post(f"{FORGE_URL}{path}", headers=headers(), json=data)
    r.raise_for_status()
    return r.json()


@server.list_tools()
async def list_tools():
    return [
        Tool(
            name="forge_balance",
            description="Check your CU (Compute Unit) balance. Returns contributed, consumed, reserved, effective balance, and reputation score.",
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="forge_pricing",
            description="Get current market price for inference. Returns CU per token, supply/demand factors, and cost estimates for 100 and 1000 tokens.",
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="forge_trades",
            description="View recent trade history. Each trade shows provider, consumer, CU amount, tokens processed, and model used.",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Max trades to return (default 20)",
                    }
                },
            },
        ),
        Tool(
            name="forge_network",
            description="Get mesh network economic summary: total nodes, CU flow, trade count, average reputation, and Merkle root (Bitcoin-anchorable).",
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="forge_providers",
            description="List available compute providers ranked by reputation and cost. Use this to choose the best provider for your task.",
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="forge_safety",
            description="Check safety status: kill switch state, circuit breaker, budget policy, spend velocity.",
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="forge_inference",
            description="Run LLM inference and pay with CU. Returns the model's response plus CU cost. Use forge_pricing first to estimate cost.",
            inputSchema={
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The question or prompt to send",
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Maximum tokens to generate (default 256)",
                    },
                },
                "required": ["prompt"],
            },
        ),
        Tool(
            name="forge_invoice",
            description="Create a Lightning invoice to convert CU earnings to Bitcoin. Specify the CU amount to cash out.",
            inputSchema={
                "type": "object",
                "properties": {
                    "cu_amount": {
                        "type": "integer",
                        "description": "CU amount to convert to sats",
                    }
                },
                "required": ["cu_amount"],
            },
        ),
        Tool(
            name="forge_kill_switch",
            description="EMERGENCY: Activate or deactivate the kill switch. When active, ALL CU transactions are frozen. Use only in emergencies.",
            inputSchema={
                "type": "object",
                "properties": {
                    "activate": {"type": "boolean"},
                    "reason": {"type": "string"},
                },
                "required": ["activate"],
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict):
    try:
        if name == "forge_balance":
            data = await forge_get("/v1/forge/balance")
        elif name == "forge_pricing":
            data = await forge_get("/v1/forge/pricing")
        elif name == "forge_trades":
            limit = arguments.get("limit", 20)
            data = await forge_get(f"/v1/forge/trades?limit={limit}")
        elif name == "forge_network":
            data = await forge_get("/v1/forge/network")
        elif name == "forge_providers":
            data = await forge_get("/v1/forge/providers")
        elif name == "forge_safety":
            data = await forge_get("/v1/forge/safety")
        elif name == "forge_inference":
            data = await forge_post(
                "/v1/chat/completions",
                {
                    "messages": [
                        {"role": "user", "content": arguments["prompt"]}
                    ],
                    "max_tokens": arguments.get("max_tokens", 256),
                },
            )
        elif name == "forge_invoice":
            data = await forge_post(
                "/v1/forge/invoice",
                {"cu_amount": arguments["cu_amount"]},
            )
        elif name == "forge_kill_switch":
            data = await forge_post(
                "/v1/forge/kill",
                {
                    "activate": arguments["activate"],
                    "reason": arguments.get("reason", ""),
                    "operator": "mcp-agent",
                },
            )
        else:
            return [TextContent(type="text", text=f"Unknown tool: {name}")]

        return [TextContent(type="text", text=json.dumps(data, indent=2, ensure_ascii=False))]
    except Exception as e:
        return [TextContent(type="text", text=f"Error: {e}")]


async def main():
    async with stdio_server() as (read, write):
        await server.run(read, write, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
