# Tirami

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-1574_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![Rust](https://img.shields.io/badge/rust-edition_2024-orange)]()

Tirami は、ローカル LLM サーバーに計算資源の貸し借りを組み込んだ分散推論プロトコルです。

手元のマシンで推論を動かしつつ、モデルが載らないときは他のノードに依頼できます。
依頼した分は借りになり、他のノードの推論を引き受ければ返せます。
貸し借りの単位は FLOP で、1 TRM = 10⁹ FLOP として数えます。

> 💡 **まずは 1 台で試せます。** `tirami start` だけで OpenAI 互換 API が立ち上がります。
> 経済機能は使わなければ意識する必要はありません。

## 🤔 なぜ Tirami か

ローカル LLM を選ぶ理由は、だいたい次の4つに集約されます。

- **コスト** — 従量課金は使うほど増えます。クラウド API が $5-15 / 1M tokens に対し、ローカル推論は電気代 $5-10 / 月。使用量が一定を超えれば逆転します。
- **バージョンを固定できない** — 利用者はモデルのバージョンを固定も検証もできません。予告なく更新され、非推奨化されます。ローカルの GGUF はハッシュが同じなら明日も同じものです。
- **アクセス** — 月額が払えない層、為替や地域の事情で払えない層が存在します。
- **トラストバウンダリ** — プロンプトが第三者に渡ります。規制業種や個人的な用途では、それだけで選択肢から外れます。

ここまでは [Ollama](https://ollama.com) や LM Studio が既に解いています。

**Tirami が解くのはその次にある壁です。** 32B を Q4 で動かすのに約 20GB、70B で約 40GB、
100B+ の MoE で 50GB 以上のメモリが要ります。多くの人の手元にその機械はありません。
データセンタを建てるか、小さいモデルで我慢するかの二択になります。

**Tirami はこの壁を、機械を買うのではなく融通し合うことで越えます。**

> Ollama は「ローカルでモデルを動かす」を解きました。
> Tirami が解くのは「手元に載らないモデルを、データセンタなしで動かす」です。

設計思想の全体は [`docs/pmvv.md`](docs/pmvv.md) にまとめています。

## 🚦 3 段階 — どこまで動くか

「休眠している PC を束ねて大きいモデルを動かす」は段階 2 以降の話です。
**今日動くのは段階 1 です。**

| 段階 | 内容 | 状態 | 制約 |
|---|---|---|---|
| **1** | 他ノードへ推論を依頼（転送） | ✅ **動作中** | 提供者側にモデルが載っている必要があります。プロンプトは提供者に平文で見えます |
| **2** | LAN / Thunderbolt で複数台のメモリを束ねる | 🚧 開発中（[#162](https://github.com/clearclown/tirami/issues/162) / [#163](https://github.com/clearclown/tirami/issues/163)） | 低 RTT のリンクが前提です |
| **3** | 広域でのメモリプール | 🔬 研究中 | 重み転送が RTT 律速になります（[#164](https://github.com/clearclown/tirami/issues/164) 実測） |

実機測定（Mac mini M4 Pro ⟷ Mac Studio M4 Max）で分かったのは、
**推論そのものは RTT にほとんど影響されない**一方、**重みの転送は RTT に比例して破綻する**ことです。
RTT が 9.2 倍になっても生成速度の低下は 1.36 倍ですが、17 GiB の重み転送は
Thunderbolt の 139 秒に対し有線 LAN で 867 秒かかりました。

つまり広域で成立するのは「モデルを持っている人に頼む」であって、
「レイヤーを分けて配る」ではありません。段階 3 が研究中なのはこのためです。

## 🔐 プライバシーについて

**Tirami が提供するのはプライバシーそのものではなく、トラストバウンダリの選択権です。**

段階 1 で他ノードに依頼すると、**プロンプトは実行するノードに平文で見えます**。
ここは隠しません（[`docs/faq.md`](docs/faq.md) 参照）。

価値は、選択肢が 1 つしかない状態から抜けられることです。
自分の機械で完結させる / 信頼できる相手を指名する / 匿名のピアに投げる、を用途ごとに選べます。
暗号化したまま他人に推論させる仕組み（zkML・TEE）はまだ先の段階です。

## ✨ 特長

- 🔗 **ブロックチェーンを使いません。** 取引は当事者 2 者が Ed25519 で相互署名するだけで確定します。ガス代も承認待ちもブロック生成時間もありません。台帳は HMAC-SHA256 で改竄を検出します。
- 🔑 **IP アドレスを知らなくても接続できます。** ノードの識別子は公開鍵（Ed25519）です。相手の居場所は [iroh](https://github.com/n0-computer/iroh) の分散 discovery が解決し、NAT の内側同士でもリレー経由で直接つながります。固定 IP・ポート開放・VPN は不要です。
- 🔌 **OpenAI 互換 API。** 既存のクライアントやライブラリをそのまま使えます。応答には `x_tirami` フィールドが付き、その推論に何 TRM かかったかが分かります。
- 📊 **TRM は投機トークンではありません。** 計算量そのものの単位（1 TRM = 10⁹ FLOP）です。ICO・プレマイン・運営取り分・エアドロップはいずれもありません。総供給量は 21B TRM です。
- 🖥 **主要な環境で動きます。** macOS（Apple Silicon は Metal が有効）、Linux。推論エンジンは llama.cpp、モデルは GGUF 形式です。
- 🧪 **1,574 件のテストが通っています。** macOS arm64 × 2 と Linux x86_64 × 2 の計 4 ホストで、2 シード + 35 ワーカーを 24 時間以上動かして検証しています。

## 📦 インストール

```bash
git clone https://github.com/clearclown/tirami
cd tirami
cargo build --release
```

GPU を使う場合はビルド時に選びます。

```bash
cargo build --release -p tirami-cli --features tirami-infer/cuda    # NVIDIA
cargo build --release -p tirami-cli --features tirami-infer/metal   # Apple Silicon
```

macOS では feature なしでも Metal が有効になります（`llama-cpp-sys-2` のビルドスクリプトによる）。
詳細は [`docs/operator-guide.md`](docs/operator-guide.md) をご覧ください。

## 🚀 使い方

### 1 台で動かす

```bash
./target/release/tirami start
```

モデルの取得、鍵の生成、API の起動まで自動で行われます。

```console
$ curl -s localhost:3000/v1/chat/completions \
    -H 'Content-Type: application/json' \
    -d '{"model":"qwen2.5:1.5b","messages":[{"role":"user","content":"こんにちは"}]}' \
  | jq '{content: .choices[0].message.content, x_tirami}'

{
  "content": "こんにちは！何かお手伝いできることはありますか？",
  "x_tirami": {
    "trm_cost": 47,
    "effective_balance": 953
  }
}
```

利用できるモデルは `smollm2:135m` `qwen2.5:0.5b`（既定）`1.5b` `3b` `7b` `14b` `32b` です。
任意の GGUF もローカルパス・HuggingFace URL・`org/repo/file.gguf` の形式で指定できます。

### ネットワークに参加する

モデルを持つ側（提供者）は、起動時に自分の公開鍵を表示します。

```console
$ ./target/release/tirami start --model qwen2.5:32b
Node ID: 3f8a1c4e...9e2b
```

借りる側はその公開鍵を指定するだけです。

```bash
./target/release/tirami start --bootstrap-peer 3f8a1c4e...9e2b
```

手元に 32B のモデルが無くても API は同じように使えます。
推論は提供者側で実行され、生成されたトークン量に応じた TRM が双方の署名付きで記録されます。

新規ノードには 1,000 TRM の無利子ローン（72 時間）が付きます。
提供者として継続的に稼ぐには一定額のステークが必要です（Sybil 対策）。

詳しい運用は [`docs/operator-guide.md`](docs/operator-guide.md) をご覧ください。

## 🚧 既知の未実装

- **複数のマシンのメモリを束ねる機能**（段階 2）。プロトコル定義と受信側は実装済みで、送信側のオーケストレーションが未着手です（[#162](https://github.com/clearclown/tirami/issues/162) / [#163](https://github.com/clearclown/tirami/issues/163)）。最優先で取り組んでいます。
- **同一の所有者に属するノード間の取引を相殺してゼロにする仕組み。** 自宅の複数台をつないだだけで残高が動かないようにする予定です。
- **暗号化したままの推論**（zkML / TEE）。現状は `MockBackend` のみです。

その他の実装状況は [`docs/release-readiness.md`](docs/release-readiness.md) に整理しています。

## 🏗 構成

```
crates/
├── tirami-cli      CLI
├── tirami-node     ノード本体・HTTP API・P2P パイプライン
├── tirami-net      iroh QUIC・discovery・クラスタ
├── tirami-proto    ワイヤプロトコル
├── tirami-infer    llama.cpp 推論エンジン
├── tirami-ledger   台帳・ステーク・価格・ガバナンス
├── tirami-shard    レイヤー分割の計画
└── tirami-sdk      Rust クライアント SDK
```

```mermaid
flowchart LR
  A["消費者"] -->|"① 推論を依頼"| B["提供者"]
  B -->|"② トークンを返す"| A
  A -->|"③ 双方が署名した取引記録"| B
  B -.->|"④ ゴシップで伝播"| C["周辺ノード<br/>評判・価格形成"]
```

アーキテクチャの詳細は [`docs/architecture.md`](docs/architecture.md)、
経済モデルの設計は [`docs/whitepaper.md`](docs/whitepaper.md) をご覧ください。

## 🤝 貢献

Issue や Pull Request を歓迎しています。以下のような報告はとくに助かります。

- 経済モデルへの攻撃方法の指摘（「こうすれば TRM を不正に増やせる」というご指摘はいちばんありがたいです）
- P2P 接続が確立できなかった環境の報告（NAT の種類やネットワーク構成を添えていただけると助かります）
- お使いのハードウェア・OS・モデルの組み合わせでの動作報告

開発の手引きは [`AGENTS.md`](AGENTS.md)、セキュリティに関する報告方法は [`SECURITY.md`](SECURITY.md) にまとめています。

## 🌏 翻訳

過去に11言語の翻訳がありますが、いずれも Phase 19 時点の内容で更新が止まっています。
[`docs/translations/`](docs/translations/) にありますが、最新は本ファイルです。

## 🙏 謝辞

推論基盤は [mesh-llm](https://github.com/Mesh-LLM/mesh-llm)（Michael Neale 氏が開始し、現在は Mesh-LLM organization で保守）に由来します。
Tirami はその上に経済層を実装したものです。詳細は [CREDITS.md](CREDITS.md) をご覧ください。

## 📄 ライセンス

MIT License
