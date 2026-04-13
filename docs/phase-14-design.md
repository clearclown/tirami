# Phase 14 設計書 — 統一スケジューラ・信頼グラデーション・ユーザ参加

> Version 0.1 — 2026-04-14
>
> v2 (`~/Projects/tirami-v2`) の参照実装で実証された設計原理を v1 へ段階的に取り込む。
> 経済合理性・ユーザ参加・セキュリティの3観点から再設計する。

---

## 1. 背景と目的

### 1.1 v1 の現状 (2026-04-14)

v1 は Phase 1-13 を経て 785 テストが通る実用段階に到達している。
本日の2台実機テスト (Seed `100.112.10.128:3030` + Worker) で以下が確認された:

- P2P 接続 (iroh QUIC + Noise) が Tailscale 越しで動作
- llama.cpp で実際の推論が実行される (Qwen2.5-0.5B)
- 双方署名取引 (Ed25519) がゴシップ伝搬
- Bank L2 が自動で lending 判断 (high-yield strategy で 4,000 TRM 配分)

ただし次の根本的な断絶が残る:

1. **推論スケジューリングと経済決定が別ループ** — pipeline coordinator が `llama-cli --rpc` を呼んだ後に `execute_trade` を後付けで呼ぶ。レジャーの価格・レピュテーションは推論の配分に影響しない
2. **プロバイダ選択が市場シグナルに基づかない** — 静的トポロジーが決め、価格競争が存在しない
3. **「手抜き推論 (lazy provider)」に対する明示的な対策がない** — 閾値 T4 は threat-model.md で「accepted risk」とされている
4. **新規ユーザの参加障壁が高い** — Rust ビルド・CLI・Ed25519 鍵管理を要求

### 1.2 v2 参照実装で実証された設計

`~/Projects/tirami-v2` (7 crate, 445 tests, 17MB binary) で以下が動作確認された:

- **Ledger-as-Brain**: `select_provider + reserve + settle` が原子操作 (`InferenceTicket` パターン)
- **PeerRegistry**: 各ノードの price_multiplier/latency_ema/audit_tier を集約
- **AuditTier 自動昇格**: Unverified → Probationary (verified_trades=2 で到達)
- **PriceSignal 自動ブロードキャスト**: gossip loop が30秒ごとに価格表明
- **真の bilateral trade**: consumer_id が匿名 (`0xff...`) ではなく実 NodeId

### 1.3 Phase 14 のスコープ

v1 のコードベース (14 crate, 785 tests) を保守しつつ、v2 の設計原理を段階的に取り込む。**v1 を置き換えるのではなく進化させる**。

- ✅ 本番機能 (Lightning, Agora L4, Bank L2, Mind L3, SDK, MCP) はすべて維持
- ✅ 既存テストはすべて通ること
- ✅ 既存 API の後方互換性を保つ

---

## 2. 三つの設計レンズ

### 2.1 経済合理性 (Economic Rationality)

**問い**: 合理的なエージェント/人間が Tirami に参加するのは、なぜか？その動機が **v1 の現状で成立しているか**？

#### 現状の経済誘因 (v1 で動いているもの)

tirami-economics の game-theory.md §2.1 より:

```
E[π_join_now] - E[π_wait_1] = yield_0 × T_0 > 0
```

早期参加が支配戦略。これは数学的に正しい。しかし実装面で以下が不足:

| 誘因 | v1 の現状 | 不足 |
|------|----------|------|
| プロバイダが TRM を稼ぐ | ✅ 推論実行 → contributed +X | **価格差で勝つ手段がない** (全プロバイダ同一価格) |
| コンシューマが安く買う | ✅ welcome loan 1,000 TRM で bootstrap | **プロバイダ選択肢がない** (pipeline が1つ決める) |
| ステーカーが利回り | ✅ Phase 13 で実装 | availability_yield のみ、**routing 優先度が機能していない** |
| 信頼できるノードがリターン | ✅ 双方署名でレピュテーション蓄積 | **具体的な経済的見返りが薄い** (reputation × price_adjusted_cost は routing で使われていない) |

#### Phase 14 で追加する経済誘因

**A) プロバイダ間の価格競争 (PriceSignal)**

各プロバイダが自分の `price_multiplier` を 30 秒ごとに gossip で表明する。
- `0.8` → ディスカウント (手すきなので安く売りたい)
- `1.0` → 標準価格
- `1.5` → 混雑中 (高負荷なので高く売る)

これにより:
- **価格発見が動的** になる — EMA だけでなくノード単位で発見
- **プロバイダが自分の稼働率を市場に伝える** ことができる
- **コンシューマが最安ノードを選ぶ** インセンティブ

**B) レピュテーション連動ルーティング (select_provider)**

`score = effective_reputation × (1/price_multiplier) × (1/(1+latency/1000)) × capacity_ratio`

高レピュテーション + 低価格 + 低レイテンシのノードが優先される。これが:
- **プロバイダが信頼を積む経済的見返り** になる (レピュテーション上昇 → 仕事が来る → 収入増)
- **コンシューマは客観スコアで選べる** (市場価格は賢明な買い手を仮定しない)

**C) 監査合格でのレピュテーション昇格 (AuditTier)**

`Unverified → Probationary → Established → Trusted → Staked` で監査頻度が 100% → 0.1% に低下。

経済的意味:
- 新規ノードは監査コスト (冗長実行) を負担 → **参入期の経済的負荷**
- 信頼を積むと監査コストが減る → **信頼 = 将来のコスト削減**
- ステーキングが最速の信頼取得ルート → **ステーキングの routing-優先度以外の実用価値**

これで「なぜステーキングするか？」の合理的答えが routing-優先度だけでなく「監査免除」にも広がる。

### 2.2 ユーザ参加 (User Participation)

**問い**: Tirami の参加者は誰か？どうすれば増やせるか？

#### 三つのユーザ層

| 層 | 特徴 | 参加形態 | v1 の状況 |
|----|------|----------|-----------|
| **開発者/運用者** | Rust 環境あり、技術的 | `tirami seed` で自分のノードを立てる | ✅ 動く (ただし cmake + ビルド必須) |
| **AI エージェント** | プログラム、API 経由 | `/v1/chat/completions` + `X-Tirami-Node-Id` | ✅ Phase 13 で対応済み |
| **一般利用者** | CLI 苦手、ブラウザ使う | ホスト型ゲートウェイ経由 | ❌ **存在しない** |

v2 も同じ状態 — CLI + bearer token 止まり。

#### Phase 14 で追加する参加導線

**A) `tirami init` コマンド**

```bash
$ tirami init
Generated node identity: tirami_7bc98f64...
Welcome loan received: 1,000 TRM (0% interest, 72h term)
HTTP API available at http://127.0.0.1:3000
Node is now earning. Run `tirami status` to check balance.
```

内部で以下を自動実行:
1. Ed25519 鍵を `~/.tirami/node.key` に生成
2. 設定ファイル `~/.tirami/config.toml` を生成
3. welcome loan を自動発行 (ローカルレジャー)
4. HTTP API サーバーを起動
5. モデルを指定すれば `--model qwen2.5:0.5b` で seed モードに昇格

**B) 人間可読なノード名 (governance 経由)**

`ens` 風のネームサービス:
```
alice.tirami → tirami_7bc98f64...
bob-gpu.tirami → tirami_d206a949...
```

`POST /v1/tirami/governance/reserve-name` でガバナンス経由で名前を予約。ステーキング 100 TRM が必要 (squatting 防止)。

**C) ホスト型ゲートウェイ (非技術ユーザ向け、将来)**

本 Phase ではスコープ外だが、設計として以下を想定:
- 運営者が管理するゲートウェイノードが一般ユーザからの HTTP リクエストを受ける
- ユーザは鍵もノードも持たず、OAuth でゲートウェイにログインするだけ
- ゲートウェイがその場で welcome loan を発行し、代理でリクエストを実行

これは Phase 15+ で実装する。

### 2.3 セキュリティ (Security)

**問い**: 攻撃者は何ができるか？それをどう防ぐか？

#### threat-model.md を Phase 14 観点で再評価

| 脅威 | 現状 | Phase 14 での対策 |
|------|------|-------------------|
| T1-T3 (transport/Sybil) | ✅ QUIC+Noise + rate limiting + Ed25519 | そのまま維持 |
| **T4 (Byzantine / lazy provider)** | ⚠️ **accepted risk** | **AuditTier + 挑戦/応答監査を実装** |
| T5 (MITM on relay) | ✅ end-to-end encryption | そのまま |
| T10 (TRM forgery) | ✅ dual-signed trades | そのまま |
| T11 (free-tier abuse) | ✅ Sybil 閾値 + welcome loan 返済義務 | そのまま |
| T12 (ledger divergence) | ✅ gossip + merkle root | Phase 14.5 で Bitcoin anchor 強化 (スコープ外) |
| T13 (market manipulation) | ⚠️ local price | **PriceSignal gossip でクロスノード価格発見** |
| T14 (inference quality) | ⚠️ accepted risk | **AuditTier が対処** |
| T15 (loan default) | ✅ 3:1 LTV + 30% reserve + circuit breaker | そのまま |

#### T4 (Lazy Provider) 対策の詳細

**攻撃シナリオ**: Provider がリクエストを受け取ったが、実際には計算せず、適当なトークンを返す。TRM は獲得するが計算コストを支払っていない。

**対策: AuditTier + Challenge-Response 監査**

```
AuditTier 階層:
  Unverified  (新規): 毎リクエスト監査 (100%)
  Probationary:       50% 監査 (~10 取引まで)
  Established:        10% 監査 (10-100 取引 + rep > 0.6)
  Trusted:            1% 監査 (100+ 取引 + rep > 0.8)
  Staked:             0.1% 監査 (アクティブステーク保有)
```

**Challenge-Response プロトコル**:

1. **Commit-Reveal**: 監査実行者 (challenger) はまず入力と期待出力のハッシュをコミット (署名付き)。監査対象 (target) が応答する前にコミットが送信される
2. **Deterministic inference**: 決定論的設定 (temperature=0, fixed seed) で target が推論実行、結果のハッシュを返す
3. **Verification**: challenger が reveal したコミットと target の応答ハッシュを比較
4. **Verdict**: 不一致なら AuditTier が1段階降格 + trust_penalty 累積

**監査失敗時の経済的コスト**:

```
slash_rate_minor    = 5%   (trust_penalty 0.1-0.2)
slash_rate_major    = 20%  (trust_penalty 0.2-0.4)
slash_rate_critical = 50%  (trust_penalty 0.4-0.5)
```

ステーク保有者のみ slash されるため、**低信頼ノードは監査頻度が高い ≒ 低信頼ノードは大量の監査を通過する必要がある**。監査合格を偽装するのは決定論的推論のハッシュなので暗号学的に困難。

**監査コスト問題の解決**:

- 監査は challenger の自発的行為 (報酬なし、コスト負担)
- ただし、challenger は新規 provider を選別する **経済的利害** がある (low-rep を早期に弾けば自分が選んだ provider のエラー率が下がる)
- 一部を **welcome loan** の条件に組み込む: ローン返済中のノードは毎日 N 件の監査を実施する義務 (ネットワークへの貢献)

#### 新たな脅威: 監査プロトコル自体への攻撃

| 脅威 | 攻撃方法 | 対策 |
|------|----------|------|
| T18 (偽監査結果) | challenger が故意に誤判定を下す | commit-reveal で challenger も事前コミット、不一致発覚なら **challenger が slash** |
| T19 (監査回避) | target が challenger を識別して正直に応答 | challenger は匿名化 (署名のみ検証、identity は隠す) |
| T20 (結託監査) | challenger と target が結託して slash を回避 | Tarjan SCC 検出 + 複数 challenger からの verdict 要求 |

T18 の「challenger 側 slash」が重要。これにより **監査者も正直であることを強制** される。

---

## 3. 実装計画

Phase 14 を **4 つのサブフェーズ** に分割する。各サブフェーズは独立してマージ可能。

### Phase 14.1 — PeerRegistry + PriceSignal (経済合理性の基盤)

**目的**: クロスノード価格発見と市場シグナル集約。

**変更点**:

1. `tirami-core/src/types.rs`:
   - `PriceSignal` 型を追加
   - `AuditTier` enum を追加 (実装は Phase 14.3)

2. `tirami-ledger/src/peer_registry.rs` (新規):
   - `PeerRegistry` 構造体 + `PeerState`
   - `ingest_price_signal`, `providers_for_model`, `update_latency`

3. `tirami-ledger/src/ledger.rs`:
   - `ComputeLedger` に `peer_registry: PeerRegistry` フィールド追加
   - `ingest_price_signal()` メソッド追加

4. `tirami-proto/src/messages.rs`:
   - `Payload::PriceSignalGossip(PriceSignal)` variant 追加

5. `tirami-net/src/gossip.rs`:
   - `broadcast_price_signal()` 関数追加
   - 受信側 `handle_price_signal_gossip()` 追加

6. `tirami-node/src/node.rs`:
   - 定期的 (30秒) に PriceSignal をブロードキャストするタスク
   - 起動時に自身を PeerRegistry に登録

**成功基準**: 既存785テスト通過 + 新規10+テスト通過 + 2台実機で `/v1/tirami/peers` が両ノードの PriceSignal を返す。

### Phase 14.2 — select_provider + InferenceTicket (統一スケジューリング)

**目的**: 推論スケジューリングと経済決定を原子操作にする。

**変更点**:

1. `tirami-core/src/types.rs`:
   - `InferenceTicket` 構造体追加

2. `tirami-ledger/src/ledger.rs`:
   - `select_provider(&self, model_id, estimated_tokens, consumer) -> Option<(NodeId, u64)>`
   - `begin_inference(&mut self, ...) -> Result<InferenceTicket, TiramiError>` (select_provider + reserve_cu 原子操作)
   - `settle_inference(&mut self, ticket, actual_tokens, latency_ms, audit_passed) -> Result<TradeRecord, TiramiError>`

3. `tirami-node/src/pipeline.rs`:
   - 既存 `request_inference` を `begin_inference` 呼び出しに置き換える
   - レイテンシ計測を `settle_inference` に渡す

4. `tirami-node/src/api.rs`:
   - `/v1/chat/completions` ハンドラを新 API に接続

**成功基準**: 既存のチャット API が新フローで動作 + 新規15+テスト通過。

### Phase 14.3 — AuditTier + Challenge-Response (セキュリティ)

**目的**: T4 (lazy provider) 対策 + 信頼グラデーション。

**変更点**:

1. `tirami-core/src/types.rs`:
   - `AuditTier` の実装と probability 計算

2. `tirami-ledger/src/audit.rs` (新規):
   - `AuditChallenge`, `AuditResponse`, `AuditVerdict` 構造体
   - `AuditTracker` — 進行中監査の管理
   - `select_audit_targets()` — 確率的監査対象選出

3. `tirami-proto/src/messages.rs`:
   - `Payload::AuditChallenge`, `Payload::AuditResponse` 追加

4. `tirami-infer/src/engine.rs`:
   - `generate_audit(input_tokens) -> [u8; 32]` — 決定論的推論 + ハッシュ

5. `tirami-ledger/src/staking.rs`:
   - `slash_for_audit_failure(target, trust_penalty)` 拡張

6. `tirami-net/src/gossip.rs`:
   - 監査 challenge/response のゴシップ

7. `tirami-node/src/node.rs`:
   - 定期的 (60秒) に監査タスク実行

**成功基準**: 2台実機で「ダミーのlazy provider」に対して実際に slash が発生することを確認。新規20+テスト通過。

### Phase 14.4 — `tirami init` + ユーザ参加導線

**目的**: 非開発者を含む参加者層の拡大。

**変更点**:

1. `tirami-cli/src/main.rs`:
   - 新コマンド `tirami init [--model <name>]`
   - 鍵生成、設定ファイル作成、welcome loan、API 起動を1コマンドで実行

2. `tirami-ledger/src/governance.rs`:
   - `reserve_name(name, stake)` — ガバナンス経由の名前予約
   - `resolve_name(name) -> Option<NodeId>` — 名前解決

3. `tirami-node/src/api.rs`:
   - `POST /v1/tirami/names/reserve`
   - `GET /v1/tirami/names/{name}`

4. `docs/getting-started.md` (新規):
   - 3分で参加できるガイド

**成功基準**: `tirami init` 実行のみで API が起動し welcome loan が反映される。新規8+テスト通過。

---

## 4. リスクとトレードオフ

### 4.1 Phase 14.1 のリスク

- **PriceSignal の氾濫**: gossip 頻度が高すぎるとネットワーク帯域を食う
  - 緩和: 30秒間隔 + 変化が閾値以下なら送信しない (delta-triggered)
- **時刻同期の問題**: PriceSignal の timestamp がノード間でずれる
  - 緩和: Lamport timestamp または local_seen_at を併記

### 4.2 Phase 14.2 のリスク

- **select_provider の偏り**: 初期は高 reputation ノードが存在しないので全部 Unverified、選択が運任せ
  - 緩和: tiebreaker で (1) モデル保有 (2) 低レイテンシ (3) ランダム順に選ぶ
- **ticket の漏洩**: InferenceTicket が盗まれると他者の TRM で推論できる
  - 緩和: ticket はメモリ内のみ、exec 時に consumer 署名を要求

### 4.3 Phase 14.3 のリスク

- **決定論的推論の困難さ**: llama.cpp が完全決定論的でない可能性 (Metal/CUDA の非決定性)
  - 緩和: temperature=0 + fixed seed + 最初の N トークンのみ比較 (完全一致は求めない、類似度で判定)
- **監査コスト**: 高頻度監査が計算資源を食う
  - 緩和: AuditTier による段階的減衰 + challenger 側の経済インセンティブ

### 4.4 Phase 14.4 のリスク

- **自動 welcome loan の悪用**: 誰でも `tirami init` で 1,000 TRM を得られる
  - 緩和: 既存の Sybil 閾値 (100 unknown nodes) + IP ベース rate limiting
- **名前予約の squatting**: bots が短い名前を大量予約
  - 緩和: 最小ステーク 100 TRM + 30日以上の稼働実績が必要

---

## 5. スケジュールと優先順位

| Phase | 優先度 | 依存 | 想定実装時間 |
|-------|--------|------|--------------|
| 14.1 (PeerRegistry) | P0 | なし | 1-2 時間 |
| 14.2 (select_provider) | P0 | 14.1 | 2-3 時間 |
| 14.3 (AuditTier) | P1 | 14.2 | 3-4 時間 |
| 14.4 (tirami init) | P1 | 14.1 | 1-2 時間 |

14.1 と 14.4 は並列可能。14.2 は 14.1 に依存、14.3 は 14.2 に依存。

---

## 6. 参照

- `spec/parameters.md` — 経済定数の single source
- `docs/threat-model.md` — 脅威モデル (T1-T17)
- `docs/strategy.md` — 競争戦略
- `docs/agent-integration.md` — エージェント統合
- `~/Projects/tirami-v2/` — 参照実装
- `~/Projects/tirami-v2/docs/architecture.md` — v2 アーキテクチャ (未作成だが参考)
