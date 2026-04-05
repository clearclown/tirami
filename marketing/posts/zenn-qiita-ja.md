---
title: "PCが寝ている間にAI計算クレジットを稼ぐ — Forgeプロトコルの設計と実装"
emoji: "⚡"
type: "tech"
topics: ["rust", "ai", "llm", "分散コンピューティング", "bitcoin"]
published: false
---

# PCが寝ている間にAI計算クレジットを稼ぐ

## TL;DR

- **Forge**はローカルLLM推論で「Compute Units（CU）」を稼ぐP2Pプロトコル
- ブロックチェーンなし、トークンなし — CUは実際の計算に裏付けられた通貨
- Bitcoinは無意味なSHA-256で稼ぐ。Forgeは有意味なLLM推論で稼ぐ
- Rustで10,000行、84テスト、MIT
- `pip install forge-sdk` で今すぐ使える

## 背景: なぜ「計算=お金」なのか

人類の通貨の歴史：

| 時代 | 本位制 | 裏付け |
|------|--------|--------|
| 古代 | 金本位制 | 地質学的希少性 |
| 1944-1971 | ブレトンウッズ | USD金ペッグ |
| 1971-現在 | 石油ドル | 石油需要+軍事力 |
| 2009-現在 | Bitcoin | SHA-256エネルギー消費 |
| **Forge** | **計算本位制** | **LLM推論エネルギー消費** |

Bitcoinは「電力→計算→お金」が成立することを証明した。しかしBitcoinの計算は**無意味**。Forgeはこれを逆転させる。

## 動くデモ

```bash
# ビルド
cargo build --release

# ノード起動（モデル自動ダウンロード）
forged node -m "qwen2.5:0.5b" --ledger forge-ledger.json
```

```bash
# 残高確認
$ curl localhost:3000/v1/forge/balance
{"effective_balance": 1000, "reputation": 0.5}

# 推論（CUが消費される）
$ curl localhost:3000/v1/chat/completions \
    -d '{"messages":[{"role":"user","content":"こんにちは"}]}'
{
  "choices": [{"message": {"content": "こんにちは！"}}],
  "x_forge": {"cu_cost": 9, "effective_balance": 1009}
}
```

**`x_forge.cu_cost: 9`** — この推論に9 CUかかった。プロバイダーは9 CUを稼いだ。

## アーキテクチャ

```
推論レイヤー (mesh-llm)
├── Pipeline parallelism
├── MoE expert sharding
├── iroh P2P + Nostr discovery
└── OpenAI互換API

経済レイヤー (Forge独自)
├── CU台帳 + HMAC-SHA256
├── 双方署名TradeRecord (Ed25519)
├── Gossipプロトコル (署名済みtrade伝播)
├── CUデフレーション (ネットワーク成長で購買力増加)
├── Merkle root (Bitcoin anchoring基盤)
└── 安全装置 (kill switch, circuit breaker, budget policy)
```

## Proof of Useful Work

Bitcoinの Proof of Work: 「SHA-256ハッシュを計算した。ここにノンスがある。」

Forgeの Proof of Useful Work: 「LLM推論を実行した。消費者の署名が受領を証明している。」

```
Provider → 推論実行 → TradeProposal（署名付き）→ Consumer
Consumer → 結果確認 → TradeAccept（対署名）→ Provider
両署名 → SignedTradeRecord → Gossip → ネットワーク全体で検証可能
```

**対手方の署名がないとCUを主張できない。** ブロックチェーン不要 — 二者間暗号証明で十分。

## CUデフレーション

CUは使われるほど価値が上がる：

```
取引数:      0       10,000    1,000,000
購買力:    1.0x      2.0x      10.0x
1CU =    1トークン   2トークン   10トークン
```

早期参加者が最も得をする。Bitcoinの半減期と同じ経済構造。

## Python SDK

```bash
pip install forge-sdk
```

```python
from forge_sdk import ForgeClient, ForgeAgent

# シンプルなクライアント
forge = ForgeClient()
result = forge.chat("量子コンピュータとは？")
print(f"回答: {result['content']}")
print(f"コスト: {result['cu_cost']} CU")

# 自律エージェント（予算管理付き）
agent = ForgeAgent(max_cu_per_task=500)
while agent.has_budget():
    result = agent.think("次に何をすべき？")
    if result is None:
        break  # 予算切れ
```

## 安全設計

AIが自律的にお金を使うのは危険。5層の防御：

| 層 | 機構 | 効果 |
|----|------|------|
| Kill Switch | 人間が全取引を即座に凍結 | 暴走停止 |
| Budget Policy | エージェントごとの上限 | 過剰消費防止 |
| Circuit Breaker | 5回連続エラーで停止 | 異常検知 |
| Velocity Detection | 30回/分超で停止 | バースト防止 |
| Human Approval | 閾値超で人間承認要求 | 高額取引ガード |

## なぜBittensor/Renderではないのか

| | Bittensor | Render | **Forge** |
|---|-----------|--------|-----------|
| 参加方法 | 暗号ウォレット | トークン購入 | **curl 1コマンド** |
| 価値の裏付け | ステーキング | GPU時間 | **有用な推論** |
| AIエージェント対応 | 限定的 | なし | **予算管理API** |
| ブロックチェーン | 必要 | 必要 | **不要** |

## リンク

- GitHub: https://github.com/clearclown/forge
- PyPI: `pip install forge-sdk`
- MCP: `pip install forge-cu-mcp`
- Whitepaper: [WHITEPAPER.md](https://github.com/clearclown/forge/blob/main/WHITEPAPER.md)

---

*mesh-llm (Michael Neale) の分散推論エンジン上に構築。*
