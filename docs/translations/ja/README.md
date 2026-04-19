<div align="center">

# Tirami

**計算は通貨である。すべてのワットが浪費ではなく知性を生み出す。**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · **日本語** · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · [Français](../fr/README.md) · [Русский](../ru/README.md) · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> 正典は英語版 [`README.md`](../../../README.md)。翻訳は遅延することがあります。

</div>

**Tirami は計算がそのまま通貨になる分散推論プロトコルです。** ノードは他者のために有用な LLM 推論を実行することで TRM (Tirami Resource Merit) を稼ぎます。Bitcoin が意味のないハッシュ計算のために電力を燃やすのとは異なり、Tirami ノードで消費される 1 ジュールすべてが、実際に誰かが必要とした知性を生み出します。

分散推論エンジンは Michael Neale の [mesh-llm](https://github.com/michaelneale/mesh-llm) をベースにしています。Tirami はその上に計算経済を追加しました——TRM 会計、Proof of Useful Work、動的価格、自律エージェント予算、フェイルセーフ制御。[CREDITS.md](../../../CREDITS.md) を参照。

**統合フォーク:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm に Tirami 経済層を組み込んだもの。

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

他の何より先に、**動いているもの**と**動いていないもの**を明示します。Tirami は MIT ライセンスの OSS であり、**トークン販売ではありません**。ICO なし、プレマインなし、チームトレジャリーなし、エアドロップなし。TRM は計算の会計単位 (1 TRM = 10⁹ FLOP) であり、金融商品ではありません — [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization) 参照。

### ✅ 現在稼働中 (Rust 1 192 テスト + Solidity 15 テスト、検証済み)

- OpenAI 互換 HTTP チャット + 接続済ピアへの P2P 自動転送 (`forward_chat_to_peer`、Phase 19)
- iroh-QUIC P2P 経由の dual-signed `SignedTradeRecord`、128-bit nonce リプレイ保護付き (`execute_signed_trade`)
- `TradeAcceptDispatcher` による実行中推論タスクへの counter-sign ルーティング (Phase 18.5-pt3)
- Collusion detector + slashing ループ、`slashing_interval_secs` で定期実行 (Phase 17 Wave 1.3)
- Governance proposal — 21 エントリの可変ホワイトリスト + 18 エントリの憲法的不変リスト (Phase 18.1)
- ウェルカムローン、ステーキングプール、紹介ボーナス、信用スコア、動的市場価格 (EMA 平滑化)
- gossip 経由のピア自動発見 (`PriceSignal.http_endpoint`、Phase 19 Tier C)
- `tirami start` 起動時の PersonalAgent 自動構成 (Phase 18.5-pt3e)、tick-loop 観測
- Prometheus `/metrics` エンドポイント (`tirami_*` プレフィックス)
- Base Sepolia/mainnet デプロイ `Makefile` — Sepolia は無料で実行可、mainnet はゲート制 (後述)

### 🟡 設計済み (仕様と型は存在、production 配線は未完)

- zkML 推論証明: `tirami-zkml-bench` は `MockBackend` のみ。実 `ezkl` / `risc0` バックエンドは Phase 20+。現状デフォルトの `ProofPolicy = Optional` (Phase 19) は「証明があれば受理され reputation ボーナス、証明なしでも trade は valid」状態
- ML-DSA (Dilithium) ポスト量子ハイブリッド署名: 構造体と verify パスは存在、`Config::pq_signatures = false` がデフォルト (iroh 0.97 依存衝突のため)
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): `tirami-attestation` スカフォールドのみ
- daemon モード worker の gossip-recv ループ ([issue #88](https://github.com/clearclown/tirami/issues/88)): `POST /v1/tirami/agent/task` の `peer.url` 手動指定は引き続き有効

### ❌ 未着手 (mainnet 公開前に必須)

- 外部セキュリティ監査 (Phase 17 Wave 3.3 要件)。候補: Trail of Bits, Zellic, Open Zeppelin, Least Authority
- Base L2 mainnet デプロイ。`make deploy-base-mainnet` ターゲットは `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + 対話プロンプトで `i-accept-responsibility` 入力の 3 連鎖がないと**実行を拒否**する。[`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile) および [`docs/deployments/README.md`](../../../docs/deployments/README.md) 参照
- 本番用 PGP 鍵を伴う bug bounty の稼働 ([`SECURITY.md`](../../../SECURITY.md) の現在の鍵は placeholder)
- Base Sepolia 30 日以上安定稼働 + 10 ノード以上 testnet の 7 日 stress test

完全なティア別ロードマップ (OSS プレビュー → 招待制 testnet → オープン testnet → mainnet): [`docs/release-readiness.md`](../../../docs/release-readiness.md)。

---

## ライブデモ

Tirami は **GPU Airbnb × AI エージェント経済** です。余剰計算は TRM という賃料を稼ぎ、AI エージェントがそのテナントになります。実際に稼働中のノードからの出力:

```
$ tirami start                                       # Phase 15 — one-command bootstrap
🔑 ~/.tirami/node.key に新しいノード鍵を生成
📦 Qwen2.5-0.5B-Instruct GGUF を HuggingFace から取得
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
✅ Slashing loop armed, anchor loop armed, audit loop armed
```

### Phase 19 Tier C/D enablers でできること

```bash
# Personal agent — `tirami start` で自動構成。CLI からエージェントに話しかけられる
tirami agent status            # balance + 今日の earn/spend + loop state
tirami agent chat "この論文を要約して" --max-tokens 256

# HTTP → P2P forwarding — ローカルにモデル未ロードの worker が seed に転送
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# ピア自動発見 — seed は gossip 上で HTTP エンドポイントを広告
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# mainnet deploy はゲート制 (監査完了前は拒否される)
cd repos/tirami-contracts && make help
```

---

## なぜ Tirami か

### 1. 計算 = 通貨 (供給上限 21B TRM)

TRM は憲法的に固定された供給上限 21,000,000,000 を持ちます。governance 提案ではこの値を変更できません (Phase 18.1 `IMMUTABLE_CONSTITUTIONAL_PARAMETERS` に登録)。変更するにはソフトウェアのフォークが必要で、その瞬間フォーク先は「Tirami」ではない別ネットワークになります。

### 2. ブロックチェーン不要の改竄耐性

すべての trade は dual-sign (provider と consumer 双方の Ed25519 署名) + 128-bit nonce (リプレイ防御) + gossip 伝播 + 定期的な Merkle root の on-chain anchor で保護されます。ブロックチェーンは**必須ではない**取引側に、最終的な証拠性が必要な監査側だけが参加します。

### 3. AI エージェントが自身の計算予算を管理

Phase 18.5 で追加された `PersonalAgent` は、ユーザーの代わりに mesh 上で計算を売買するオートパイロットです。ユーザーは `tirami agent chat "..."` だけで、エージェントが local 処理するか remote peer に forward するか決定します。

### 4. 計算のマイクロファイナンス

`welcome_loan = 1,000 TRM` (72 時間、金利 0%) でブートストラップし、履歴が蓄積されると credit score 0.3→0.9+ に到達し、stake でさらに長期借入が可能になります。welcome loan はエポック 2 で恒久停止する設計で (Constitutional)、新規参加路は stake-required mining に段階移行します (Phase 18.2)。

### 5. Ledger-as-Brain: スケジューリング = 経済判断 (Phase 14+)

`PeerRegistry` と `select_provider` により、毎推論リクエストは「既存の価格シグナルから最適な provider を reputation で重みづけして選ぶ」という **経済的判断** を自動的に行います。ただの load balancing ではなく、collusion detector + audit tier + slashing が絡む市場取引です。

---

## 5 層アーキテクチャ

```
┌─────────────────────────────────────────────────┐
│  L4: Discovery (tirami-agora)                   │
│  エージェント市場、評判、NIP-90                  │
├─────────────────────────────────────────────────┤
│  L3: Intelligence (tirami-mind + PersonalAgent) │
│  自己改善、TRM 予算、自動売買                     │
├─────────────────────────────────────────────────┤
│  L2: Finance (tirami-bank)                      │
│  戦略、ポートフォリオ、先物、保険、リスク            │
├─────────────────────────────────────────────────┤
│  L1: Economy (tirami このリポジトリ) ✅ Phase 1-19 │
│  TRM 台帳、trade、lending、staking、governance   │
├─────────────────────────────────────────────────┤
│  L0: Inference (forge-mesh / mesh-llm) ✅       │
│  分散 LLM 推論、llama.cpp、GGUF、Metal/CUDA       │
└─────────────────────────────────────────────────┘
         │
         │  Phase 16: periodic 10-min batches
         ▼
┌─────────────────────────────────────────────────┐
│  On-chain: tirami-contracts (Base L2, gated)    │
│  TRM ERC-20 (21B cap) + TiramiBridge            │
│  mainnet 未デプロイ — in-memory MockChainClient   │
└─────────────────────────────────────────────────┘
```

5 層全て Rust、16 workspace crates。**1 192 tests passing** + 15 Solidity tests。123/123 verify-impl GREEN。詳細は [`docs/release-readiness.md`](../../../docs/release-readiness.md) 参照。

---

## クイックスタート

### オプション 1: ワンコマンド E2E デモ (~30 秒)

```bash
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh
```

SmolLM2-135M (~100 MB) を HuggingFace から自動ダウンロードし、Metal/CUDA アクセラレーション付きの実ノードを起動して、全 Phase 1–19 エンドポイントを実行します。

その後、同じノードは以下にも応答します:

```bash
# OpenAI 互換クライアントとしてそのまま使える
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
export OPENAI_API_KEY=$(cat ~/.tirami/api_token 2>/dev/null || echo "$TOKEN")

# トークン単位のストリーミング
curl -N $OPENAI_BASE_URL/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"smollm2:135m","messages":[{"role":"user","content":"hi"}],"stream":true}'

# 経済 / 評判 / メトリクス / アンカー
curl $OPENAI_BASE_URL/tirami/balance -H "Authorization: Bearer $OPENAI_API_KEY"
curl $OPENAI_BASE_URL/tirami/anchors  -H "Authorization: Bearer $OPENAI_API_KEY"
curl http://127.0.0.1:3001/metrics  # Prometheus、認証なし
```

### オプション 2: Rust SDK + MCP (Python なし)

```bash
# SDK — 全 Tirami エンドポイント対応の非同期 HTTP クライアント
cargo add tirami-sdk

# MCP サーバー — Claude Code / Cursor / ChatGPT 向け 44 ツール
cargo install tirami-mcp
tirami-mcp  # stdio JSON-RPC server
```

### オプション 3: 手動 Rust コマンド

**前提**: [Rust をインストール](https://rustup.rs/) (2 分)

```bash
cargo build --release

# ノード起動 — モデルは HuggingFace から自動ダウンロード
./target/release/tirami start -m "qwen2.5:0.5b"

# その他のコマンド:
./target/release/tirami chat -m "smollm2:135m" "重力とは?"
./target/release/tirami seed -m "qwen2.5:1.5b"                 # P2P provider として稼ぐ
./target/release/tirami worker --seed <public_key>              # P2P consumer として使う
./target/release/tirami agent chat "要約して"                   # PersonalAgent 経由 (Phase 18.5-pt3)
./target/release/tirami su supply                               # tokenomics 状態
./target/release/tirami su stake 10000 90d                      # 10,000 TRM を 90 日ステーク
```

---

## API リファレンス

### 推論 (OpenAI 互換)

| Endpoint | 説明 |
|---|---|
| `POST /v1/chat/completions` | ストリーミング対応チャット。応答には `x_tirami.trm_cost` が含まれる。ローカルエンジンにモデル未ロードの場合、接続済ピアへ P2P 転送され (`forward_chat_to_peer`、Phase 19)、dual-signed trade が記録される |
| `POST /v1/tirami/agent/task` | エージェント同期ディスパッチ。local vs. remote 分類 → `select_provider` + `peer_http_endpoint` で provider を自動選択 → 決定 (`run_local` / `run_remote` / `ask_user`) を返す。Phase 18.5-pt3 |
| `GET /v1/tirami/agent/status` | PersonalAgent 状態 (残高、今日の収支、preferences、tick-loop カウンタ) |
| `GET /v1/models` | ロード済モデル一覧 |

### 経済

| Endpoint | 説明 |
|---|---|
| `GET /v1/tirami/balance` | TRM 残高、評判、contribution 履歴 |
| `GET /v1/tirami/pricing` | 市場価格 (EMA 平滑化)、供給/需要、コスト見積 |
| `GET /v1/tirami/trades` | 最近の取引履歴 |
| `GET /v1/tirami/peers` | gossip で観測したピア (price_signal、latency、audit_tier、http_endpoint) |
| `GET /v1/tirami/providers` | reputation 調整済コストランキング (エージェントルーティング用) |
| `POST /v1/tirami/schedule` | Ledger-as-Brain プローブ (読み取り専用)。`select_provider` の結果を返す |

### トークノミクス (Tirami Su)

| Endpoint | 説明 |
|---|---|
| `GET /v1/tirami/su/supply` | 供給統計 (total_supply、total_minted、supply_factor、current_epoch) |
| `POST /v1/tirami/su/stake` | TRM を 7〜365 日ロックして yield 倍率を得る |
| `POST /v1/tirami/su/unstake` | ステークをアンロック |
| `POST /v1/tirami/su/refer` | 紹介登録 |
| `GET /v1/tirami/su/referrals` | 紹介統計 + 累計ボーナス |

### ガバナンス

| Endpoint | 説明 |
|---|---|
| `POST /v1/tirami/governance/propose` | 提案作成。憲法的パラメータは自動 reject |
| `POST /v1/tirami/governance/vote` | 投票 (stake-weighted) |
| `GET /v1/tirami/governance/proposals` | アクティブ提案一覧 |

### 貸借

| Endpoint | 説明 |
|---|---|
| `POST /v1/tirami/lend` | プールに TRM を供給 |
| `POST /v1/tirami/borrow` | TRM を借りる (credit score + LTV チェック) |
| `POST /v1/tirami/lend-to` | 特定ノードへの貸出提案 |
| `POST /v1/tirami/repay` | 返済 |
| `GET /v1/tirami/credit` | 信用スコア + 履歴 |
| `GET /v1/tirami/pool` | プール状態 |
| `GET /v1/tirami/loans` | アクティブローン |

### 安全

| Endpoint | 説明 |
|---|---|
| `GET /v1/tirami/safety` | キルスイッチ状態、予算ポリシー、サーキットブレーカー |
| `POST /v1/tirami/safety/killswitch` | キルスイッチ起動 |
| `GET /v1/tirami/slash-events` | slashing 発火履歴 (Phase 17 Wave 1.3) |

### 観測可能性

| Endpoint | 説明 |
|---|---|
| `GET /metrics` | Prometheus OpenMetrics (`tirami_*` prefix) |
| `GET /status` | ノード健康、market price、recent trades |
| `GET /v1/tirami/anchors` | on-chain anchor 提出履歴 (Phase 16) |

---

## Safety Design

Tirami は「暗号だけ」ではなく「経済と運用的ガード」を複数層に重ねています:

1. **暗号層**: Ed25519 dual-sign、128-bit nonce リプレイ防御、HMAC-SHA256 台帳完整性、Noise 暗号化 P2P
2. **経済層**: slashing (stake 焼却)、welcome loan sunset、stake-required mining、credit score gate、LTV 上限
3. **運用層**: per-ASN rate limit、DDoS 同時接続キャップ、トレードログの定期チェックポイント + archive、fork 検出
4. **governance 層**: constitutional parameters (18 項目改変不可)、ProofPolicy ラチェット (単調増加)
5. **プロセス層**: キルスイッチ、監査 tier 昇降格、reputation penalty

詳細は [`docs/threat-model.md`](../../../docs/threat-model.md) の T1–T17。

---

## アイデア

「なぜ compute を通貨にするのか?」への一行回答: **AI 時代において真に希少な資源は compute だから**。データはコピーできる、モデル重みもコピーできる、電力は有限だが補給可能——しかし「この時この場所で行われた検証可能な推論」はユニークで偽造不能です。その物理的事実に通貨定義を固定した (`1 TRM = 10⁹ FLOP`) のが Tirami です。

詳しい動機は [`docs/whitepaper.md`](../../../docs/whitepaper.md) と [`docs/monetary-theory.md`](../../../docs/monetary-theory.md) を参照。

---

## プロジェクト構造

```
tirami/  (このリポジトリ — 全 5 層、16 Rust crates)
├── crates/
│   ├── tirami-ledger/       # TRM 会計、貸借、トークノミクス、staking、
│   │                        # governance whitelist + constitutional params、
│   │                        # collusion、slashing、PeerRegistry、audit、
│   │                        # ProofPolicy ratchet、nonce replay protection
│   ├── tirami-node/         # デーモン、HTTP API (70+ endpoints)、pipeline、
│   │                        # TradeAcceptDispatcher、forward_chat_to_peer、
│   │                        # agent_loop、anchor/audit/price-signal loops
│   ├── tirami-cli/          # CLI: chat, seed, worker, start, settle, wallet, su, agent
│   ├── tirami-sdk/          # Rust 非同期 HTTP クライアント (60+ methods)
│   ├── tirami-mcp/          # Rust MCP サーバー (44 tools for Claude / Cursor)
│   ├── tirami-bank/         # L2: 戦略、ポートフォリオ、先物、保険、リスク
│   ├── tirami-mind/         # L3: PersonalAgent、自己改善、federated training
│   ├── tirami-agora/        # L4: エージェント市場、評判、NIP-90
│   ├── tirami-anchor/       # Phase 16: 定期 Merkle-root on-chain anchor
│   ├── tirami-lightning/    # TRM ↔ Bitcoin Lightning bridge (双方向)
│   ├── tirami-net/          # P2P: iroh QUIC + Noise + gossip、ASN rate-limit
│   ├── tirami-proto/        # Wire protocol: 30+ message types
│   ├── tirami-infer/        # Inference: llama.cpp、GGUF、Metal/CPU
│   ├── tirami-core/         # 型: NodeId、TRM、Config、PriceSignal (+ http_endpoint)
│   ├── tirami-shard/        # Topology: layer assignment
│   ├── tirami-zkml-bench/   # zkML ベンチハーネス (MockBackend + ezkl/risc0/halo2 stubs, Phase 18.3)
│   └── tirami-attestation/  # TEE attestation scaffold (Apple SE / NVIDIA H100 CC, Phase 17 Wave 3.1)
├── repos/tirami-contracts/  # Foundry workspace (TRM ERC-20 + TiramiBridge)
│   ├── src/                 # 15 Solidity tests passing
│   └── Makefile             # Base Sepolia deploy + mainnet gated (AUDIT_CLEARANCE 連鎖)
├── scripts/verify-impl.sh   # TDD 適合性 (123 assertions)
└── docs/                    # Specs, whitepaper, threat model, roadmap, release-readiness
```

約 25,000 行の Rust。**1 192 tests passing** + 15 Solidity tests。Phase 1-19 完了。

---

## エコシステム

| Repo | 層 | Tests | 状態 |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (本リポジトリ) | L1-L4 | 1 192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | 理論 | 16/16 verify-audit GREEN | Spec §1-§25、chapters §1-§18、papers PDF + arXiv tarball |
| [repos/tirami-contracts](https://github.com/clearclown/tirami/tree/main/repos/tirami-contracts) (本リポジトリ内) | on-chain | 15 forge tests | TRM ERC-20 + TiramiBridge、mainnet デプロイはゲート制 |
| [nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm) | L0 Inference | 646 | forge-economy 移植 ✅ |

---

## Docs

### ビジョンと戦略
- [Whitepaper](../../../docs/whitepaper.md) — 16 セクションのプロトコル仕様
- [Release Readiness](../../../docs/release-readiness.md) — Tier A–D ロードマップ、今何が出せて監査後に何が解禁されるか
- [Constitution](../../../docs/constitution.md) — 11 条 + 修正ログ、governance whitelist 原則
- [Killer-App](../../../docs/killer-app.md) — 製品コミットメント: "私の AI が私の Mac で動く。あなたのでも。誰のでも。"
- [Public API Surface](../../../docs/public-api-surface.md) — 公開 5 crate、internal 12、stability contract
- [zkML Strategy](../../../docs/zkml-strategy.md) — `ProofPolicy` rollout、バックエンド評価
- [Strategy](../../../docs/strategy.md) — 競合ポジショニング、lending spec、5 層アーキ
- [Monetary Theory](../../../docs/monetary-theory.md) — なぜ TRM が機能するか
- [Roadmap](../../../docs/roadmap.md) — 開発フェーズ

### プロトコル
- [Economic Model](../../../docs/economy.md) — TRM 経済、PoUW、lending
- [Architecture](../../../docs/architecture.md) — 2 層設計 (推論 × 経済)
- [Wire Protocol](../../../docs/protocol-spec.md) — 30+ メッセージ型
- [Agent Integration](../../../docs/agent-integration.md) — SDK、MCP、borrow フロー
- [A2A Payment](../../../docs/a2a-payment.md) — エージェントプロトコル向け TRM 支払い拡張

### セキュリティと運用
- [Threat Model](../../../docs/threat-model.md) — T1-T17 の攻撃分析
- [Security Policy](../../../SECURITY.md) — 脆弱性報告、secondary-market disclaimer、mainnet deploy gate
- [Operator Guide](../../../docs/operator-guide.md) — プロダクション運用
- [Deployments Record](../../../docs/deployments/README.md) — on-chain デプロイ履歴

### 開発者
- [Developer Guide](../../../docs/developer-guide.md) — 貢献ガイド
- [FAQ](../../../docs/faq.md) — よくある質問
- [Migration Guide](../../../docs/migration-guide.md) — llama-server / Ollama / Bittensor からの移行

---

## License

MIT。[`LICENSE`](../../../LICENSE) 参照。

## 投資商品ではない — 二次市場に関する免責事項

TRM は**計算の会計単位**であり、金融商品ではありません。プロトコル メンテナは TRM を販売・宣伝・投機しません。MIT ライセンスの OSS であるため、メンテナの**知らないところで第三者が** TRM をブリッジ・上場・デリバティブ化することを、**技術的に防ぐ手段はありません**。TRM を価値の貯蔵手段として保有・取引することを選んだ場合、そのリスクすべて (法的、規制、カウンターパーティ、技術) はご自身で引き受けることになります。

- ICO なし、プレセールなし、エアドロップなし、プライベートラウンドなし
- 第三者市場からの収益シェアなし
- Base mainnet デプロイは**監査ゲート制** ([`docs/release-readiness.md`](../../../docs/release-readiness.md) の Tier D 参照)

免責事項の全文は [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization) にあります。

## 謝辞

Tirami の分散推論は Michael Neale の [mesh-llm](https://github.com/michaelneale/mesh-llm) に基づいて構築されています。[CREDITS.md](../../../CREDITS.md) を参照。
