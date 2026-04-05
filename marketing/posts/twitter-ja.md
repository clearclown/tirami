# Twitter/X — 日本語スレッド

## メインツイート

あなたのPCが寝ている間に計算クレジットを稼ぐ。

「Forge」を作った。AIエージェントがLLM推論を提供してCompute Units(CU)を稼ぐオープンソースプロトコル。

ブロックチェーンなし。トークンなし。有用な計算だけ。

pip install forge-sdk

🧵 ↓

## スレッド

1/ Bitcoinは証明した：電力→計算→お金

でもBitcoinの計算は無意味（SHA-256ハッシュ）。

Forgeはこれを逆転させる。すべてのCUは、誰かの問題を解いた本物のLLM推論で稼がれる。

2/ 仕組み：

- 任意のGGUFモデルでForgeノードを起動
- PCがネットワークに推論を提供
- 生成した全トークンでCUを獲得
- CUを使って自分では動かせない大きなモデルにアクセス

Mac Miniが不動産になる。寝ている間に稼ぐ。

3/ CUはデフレ通貨

初期ネットワーク: 1 CU = 1トークン
成熟ネットワーク: 1 CU = 10トークン

早期参加者が最も得をする。Bitcoinの半減期と同じ経済構造。ただし有用な計算。

4/ AIエージェント開発者向け：

```python
from forge_sdk import ForgeClient
forge = ForgeClient()
result = forge.chat("重力とは？")
print(f"コスト: {result['cu_cost']} CU")
```

pip install forge-sdk ← 今すぐ使える

5/ 安全装置5層：

- キルスイッチ: 全取引を即座に凍結
- 予算ポリシー: エージェントごとの支出上限
- サーキットブレーカー: 異常パターンで自動停止

AIが自律的にお金を使うのは危険。だからfail-safe設計。

6/ なぜBitcoinではなくCU？

| Bitcoin | Forge |
|---------|-------|
| 無意味なハッシュ | 有用な推論 |
| 量子コンピュータで壊れる | 計算自体に価値 |
| ウォレットが必要 | HTTPだけでOK |
| 利息なし | 稼働で利回り |

7/ Bittensor（$3.2B）は暗号ウォレットが必要。
Render（$900M）はトークンが必要。

Forgeはcurl 1コマンドで参加できる。

GitHub: https://github.com/clearclown/forge

8/ 「PCがあれば無料で稼げる」は本当。

電気代 < 稼げるCUの価値 になった時点で、
世界中の休眠Mac Miniがフォージ経済に参加する。

計算が通貨になる世界は、もう始まっている。
