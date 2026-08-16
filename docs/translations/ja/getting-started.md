# はじめての Forge — 手を動かして理解するガイド

> ⚠️ **この文書は改名前のものです。** プロジェクト名は Tirami、単位は TRM
> (1 TRM = 10⁹ FLOP) です。本文中の「Forge」「CU」はそれぞれ読み替えてください。
> 最新の入門は [`README.md`](../../../README.md)、設計思想は
> [`docs/pmvv.md`](../../pmvv.md) にあります。

> プログラミング初心者でも、このガイドに沿って進めれば
> 10 分以内に Forge ノードが動き、AI が CU を稼ぐところまで体験できます。

---

## 前提条件

| 必要なもの | 確認方法 |
|---|---|
| Mac (M1 以降) or Linux PC | — |
| Rust ツールチェーン | `rustc --version` → 1.80 以上ならOK |
| Git | `git --version` |
| ネット接続 | 初回のみ (モデルダウンロード用、約 100 MB) |

**Rust が入っていない場合:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

2 分で終わります。

---

## ステップ 1: ソースコードを取得する

```bash
git clone https://github.com/clearclown/forge
cd forge
```

---

## ステップ 2: ビルドする

```bash
cargo build --release
```

初回は 2-3 分かかります。2 回目以降は数十秒です。

ビルドが終わると `./target/release/forge` にバイナリができます。

**確認:**

```bash
./target/release/forge --help
```

`chat`, `node`, `seed`, `models` などのコマンドが表示されれば成功です。

---

## ステップ 3: どんなモデルが使えるか確認する

```bash
./target/release/forge models
```

```
Available models:
  qwen2.5:0.5b     ~491MB   ← 標準的 (日本語もそこそこ)
  qwen2.5:1.5b    ~1100MB   ← よりスマート
  smollm2:135m     ~100MB   ← 最小・最速 (精度は低い)
```

初めてなら **smollm2:135m** (100 MB) がおすすめです。
ダウンロードが速く、メモリも少なくて済みます。

---

## ステップ 4: AI と会話してみる

```bash
./target/release/forge chat -m smollm2:135m "こんにちは！今日の天気は？"
```

初回実行時にモデルが自動ダウンロードされます (約 30 秒)。
その後、AI が応答を返します。

```
こんにちは！天気は...（モデルの回答）

---
Generated in 0.45s
```

**おめでとうございます!** あなたの PC で AI が動きました 🎉

ただし、これはまだ「経済なし」のモードです。
CU の概念を体験するには、次のステップに進みます。

---

## ステップ 5: ノードを起動して CU を稼ぐ

```bash
# ターミナル 1: ノードを起動
./target/release/forge node \
  -m smollm2:135m \
  --port 3000 \
  --api-token my-secret-token
```

ノードが起動すると、HTTP サーバーが `http://localhost:3000` で待ち受けます。

```bash
# ターミナル 2: AI に質問する (HTTP 経由)
curl -X POST http://localhost:3000/v1/chat/completions \
  -H "Authorization: Bearer my-secret-token" \
  -H "Content-Type: application/json" \
  -d '{"model":"smollm2:135m","messages":[{"role":"user","content":"1+1は？"}],"max_tokens":20}'
```

レスポンスの中に `x_forge` フィールドがあります:

```json
{
  "choices": [{"message": {"content": "2です。"}}],
  "x_forge": {
    "cu_cost": 5,
    "effective_balance": 1005
  }
}
```

- `cu_cost: 5` → この質問に 5 CU 分の計算を行った
- `effective_balance: 1005` → あなたの残高が 1005 CU に増えた (初期 1000 + 5 稼いだ)

**CU が動いています!**

---

## ステップ 6: 残高と取引履歴を確認する

```bash
# 残高
curl -H "Authorization: Bearer my-secret-token" \
  http://localhost:3000/v1/forge/balance
```

```json
{
  "contributed": 5,
  "consumed": 0,
  "effective_balance": 1005,
  "reputation": 0.5
}
```

- `contributed: 5` → 推論を提供して 5 CU 稼いだ
- `reputation: 0.5` → 新規ノードの初期評判値 (0.0〜1.0)

```bash
# 取引履歴
curl -H "Authorization: Bearer my-secret-token" \
  "http://localhost:3000/v1/forge/trades?limit=5"
```

```json
{
  "count": 1,
  "trades": [{
    "provider": "0000...0000",
    "consumer": "ffff...ffff",
    "cu_amount": 5,
    "tokens_processed": 5,
    "model_id": "SmolLM2-135M-Instruct-Q4_K_M"
  }]
}
```

ここまで来たら、Forge の基本は理解できています。

---

## ステップ 7: 一括デモで全機能を確認する

全自動で Phase 1-12 の全機能を一気に試すスクリプトがあります:

```bash
bash scripts/demo-e2e.sh
```

このスクリプトは:

1. ✅ モデルを自動ダウンロード (smollm2:135m, 100 MB)
2. ✅ ノードを起動 (Metal GPU 自動検出)
3. ✅ 3 回の推論を実行 → CU が帳簿に記録される
4. ✅ 残高・取引・価格を確認
5. ✅ L2 (金融) — ポートフォリオ判断、リスク評価
6. ✅ L4 (市場) — エージェント登録・検索
7. ✅ L3 (知能) — 自己改善サイクル
8. ✅ 共謀検出 → 信頼ペナルティ 0 (正常)
9. ✅ Prometheus メトリクス (11 系列)
10. ✅ Bitcoin OP_RETURN (マークルルート書き込み準備)
11. ✅ ノードを停止

**所要時間**: 冷起動 約 30 秒 / キャッシュ済み 約 5 秒

---

## ステップ 8 (任意): Python から使う

```bash
pip install forge-sdk
```

```python
from forge_sdk import ForgeClient

# ノードに接続 (ステップ 5 のノードが起動中であること)
client = ForgeClient(
    base_url="http://localhost:3000",
    api_token="my-secret-token"
)

# 残高確認
print(client.balance())
# → {'effective_balance': 1005, 'reputation': 0.5, ...}

# ポートフォリオの投資判断を実行
print(client.bank_tick())
# → [{'action': 'lend', 'cu_amount': 4000, 'rationale': '...'}]

# マーケットプレイスでエージェントを検索
print(client.agora_find(model_patterns=["*"], max_cu_per_token=100))
```

---

## 次に読むべきドキュメント

| あなたの目的 | 読むもの |
|---|---|
| Forge が何か知りたかった | → [Forge って何？](what-is-forge.md) (このガイドの前の段階) |
| ノードを本格運用したい | → [運用ガイド](../../../docs/operator-guide.md) |
| コードに貢献したい | → [開発者ガイド](../../../docs/developer-guide.md) |
| Ollama / llama-server から移行したい | → [移行ガイド](../../../docs/migration-guide.md) |
| 経済理論を深く知りたい | → [Compute Standard 論文](https://github.com/clearclown/forge-economics/blob/main/papers/compute-standard.md) |

---

## うまくいかないとき

| 症状 | 対処法 |
|---|---|
| `cargo build` が失敗する | Rust ≥ 1.80 か確認 (`rustup update stable`) |
| モデルがダウンロードできない | ネット接続を確認。Proxy 環境なら `HTTPS_PROXY` を設定 |
| `model_loaded: false` | `-m smollm2:135m` を付けてノードを再起動 |
| Metal acceleration が効かない | Mac M1 以降であれば自動で有効。Intel Mac は CPU のみ |
| ポートが使用中 | `--port 3001` など別のポートを指定 |
| 残高が増えない | 推論リクエストを送ってください (ステップ 5 参照) |

それでも解決しない場合は [GitHub Issue](https://github.com/clearclown/forge/issues) を開いてください。
テンプレートに従って環境情報を記載していただけると、早く解決できます。

---

**ここまでお疲れ様でした。あなたは今、「計算 = 通貨」の世界への第一歩を踏み出しました。**
