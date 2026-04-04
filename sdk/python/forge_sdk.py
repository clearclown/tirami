"""
Forge Python SDK — Compute is Currency.

Usage:
    from forge_sdk import ForgeClient

    forge = ForgeClient()

    # Check balance
    balance = forge.balance()
    print(f"You have {balance['effective_balance']} CU")

    # Run inference (costs CU)
    response = forge.chat("What is gravity?")
    print(f"Answer: {response['content']}")
    print(f"Cost: {response['cu_cost']} CU")

    # Check pricing before deciding
    pricing = forge.pricing()
    cost_estimate = pricing['estimated_cost_100_tokens']
    if cost_estimate < 200:
        forge.chat("Expensive question here")

    # Agent autonomous loop
    while True:
        balance = forge.balance()
        if balance['effective_balance'] < 100:
            print("Low balance, waiting to earn more CU...")
            break
        response = forge.chat("Next task")

Install: pip install httpx
"""

from typing import Optional
import httpx
import os


class ForgeClient:
    """Client for the Forge compute economy."""

    def __init__(
        self,
        base_url: str = None,
        api_token: str = None,
        timeout: float = 30.0,
    ):
        self.base_url = base_url or os.environ.get(
            "FORGE_URL", "http://127.0.0.1:3000"
        )
        self.api_token = api_token or os.environ.get("FORGE_API_TOKEN", "")
        self._client = httpx.Client(timeout=timeout)

    def _headers(self):
        h = {"Content-Type": "application/json"}
        if self.api_token:
            h["Authorization"] = f"Bearer {self.api_token}"
        return h

    def _get(self, path: str) -> dict:
        r = self._client.get(f"{self.base_url}{path}", headers=self._headers())
        r.raise_for_status()
        return r.json()

    def _post(self, path: str, data: dict) -> dict:
        r = self._client.post(
            f"{self.base_url}{path}", headers=self._headers(), json=data
        )
        r.raise_for_status()
        return r.json()

    # ── Economy ──

    def balance(self) -> dict:
        """Get CU balance: contributed, consumed, reserved, effective_balance, reputation."""
        return self._get("/v1/forge/balance")

    def pricing(self) -> dict:
        """Get market price: cu_per_token, supply/demand factors, cost estimates."""
        return self._get("/v1/forge/pricing")

    def trades(self, limit: int = 20) -> dict:
        """Get recent trade history."""
        return self._get(f"/v1/forge/trades?limit={limit}")

    def network(self) -> dict:
        """Get mesh economic summary with Merkle root."""
        return self._get("/v1/forge/network")

    def providers(self) -> dict:
        """List providers ranked by reputation and cost."""
        return self._get("/v1/forge/providers")

    # ── Inference ──

    def chat(
        self,
        prompt: str,
        max_tokens: int = 256,
        temperature: float = 0.7,
        system: str = None,
    ) -> dict:
        """Run inference. Returns content, cu_cost, and balance.

        Example:
            r = forge.chat("What is 2+2?")
            print(r['content'])   # "4"
            print(r['cu_cost'])   # 3
            print(r['balance'])   # 997
        """
        messages = []
        if system:
            messages.append({"role": "system", "content": system})
        messages.append({"role": "user", "content": prompt})

        data = self._post(
            "/v1/chat/completions",
            {
                "messages": messages,
                "max_tokens": max_tokens,
                "temperature": temperature,
            },
        )

        return {
            "content": data["choices"][0]["message"]["content"],
            "tokens": data["usage"]["completion_tokens"],
            "cu_cost": data.get("x_forge", {}).get("cu_cost", 0),
            "balance": data.get("x_forge", {}).get("effective_balance", 0),
            "raw": data,
        }

    def can_afford(self, estimated_tokens: int) -> bool:
        """Check if you can afford a request of this size."""
        pricing = self.pricing()
        cost = int(pricing["cu_per_token"] * estimated_tokens)
        balance = self.balance()
        return balance["effective_balance"] >= cost

    # ── Safety ──

    def safety(self) -> dict:
        """Get safety status: kill switch, circuit breaker, budget policy."""
        return self._get("/v1/forge/safety")

    def kill(self, reason: str = "emergency") -> dict:
        """EMERGENCY: Activate kill switch. Freezes all CU transactions."""
        return self._post(
            "/v1/forge/kill",
            {"activate": True, "reason": reason, "operator": "python-sdk"},
        )

    def resume(self) -> dict:
        """Deactivate kill switch. Resume normal CU transactions."""
        return self._post("/v1/forge/kill", {"activate": False})

    # ── Settlement ──

    def invoice(self, cu_amount: int) -> dict:
        """Create a Lightning invoice to convert CU to Bitcoin."""
        return self._post("/v1/forge/invoice", {"cu_amount": cu_amount})

    def settlement(self, hours: int = 24) -> dict:
        """Export settlement statement for a time window."""
        return self._get(f"/settlement?hours={hours}")


class ForgeAgent:
    """Autonomous agent that manages its own compute budget.

    Example:
        agent = ForgeAgent(max_cu_per_task=500)

        while agent.has_budget():
            result = agent.think("What should I do next?")
            if result is None:
                break  # budget exhausted
            print(result['content'])
    """

    def __init__(
        self,
        base_url: str = None,
        max_cu_per_task: int = 500,
        min_balance: int = 100,
    ):
        self.client = ForgeClient(base_url=base_url)
        self.max_cu_per_task = max_cu_per_task
        self.min_balance = min_balance
        self.total_spent = 0

    def has_budget(self) -> bool:
        """Check if agent can afford another task."""
        try:
            balance = self.client.balance()
            return balance["effective_balance"] > self.min_balance
        except Exception:
            return False

    def think(self, prompt: str, max_tokens: int = 256) -> Optional[dict]:
        """Run inference if within budget. Returns None if can't afford."""
        if not self.client.can_afford(max_tokens):
            return None

        result = self.client.chat(prompt, max_tokens=max_tokens)
        self.total_spent += result["cu_cost"]

        if self.total_spent > self.max_cu_per_task:
            return None  # task budget exhausted

        return result

    def status(self) -> dict:
        """Get agent's economic status."""
        balance = self.client.balance()
        return {
            "balance": balance["effective_balance"],
            "total_spent_this_session": self.total_spent,
            "budget_remaining": self.max_cu_per_task - self.total_spent,
            "reputation": balance["reputation"],
        }
