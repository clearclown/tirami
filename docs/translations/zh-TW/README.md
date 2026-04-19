<div align="center">

# Tirami

**計算即貨幣。每一瓦特都在產生智能，而非浪費。**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · **繁體中文** · [Español](../es/README.md) · [Français](../fr/README.md) · [Русский](../ru/README.md) · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> 權威版本為英文 [`README.md`](../../../README.md)。譯文可能存在延遲。

</div>

**Tirami 是一個計算即貨幣的分散式推理協議。** 節點透過為他人執行有用的 LLM 推理來賺取 TRM (Tirami Resource Merit)。與 Bitcoin 燒電做無意義雜湊運算不同，Tirami 節點消耗的每一焦耳都在產生某人真正需要的智能。

分散式推理引擎基於 Michael Neale 的 [mesh-llm](https://github.com/michaelneale/mesh-llm)。Tirami 在其上加入了計算經濟——TRM 會計、Proof of Useful Work、動態定價、自主代理預算、失效安全控制。參見 [CREDITS.md](../../../CREDITS.md)。

**整合分支：** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — 內嵌 Tirami 經濟層的 mesh-llm。

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

在討論其他任何事之前，首先明確**現在能用**和**還不能用**的部分。Tirami 是 MIT 授權的開源軟體，**不是代幣銷售**。沒有 ICO，沒有預挖，沒有團隊金庫，沒有空投。TRM 是計算的會計單位 (1 TRM = 10⁹ FLOP)，不是金融商品——參見 [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization)。

### ✅ 當前可用 (Rust 1 192 測試 + Solidity 15 測試，已驗證)

- OpenAI 相容 HTTP 對話 + 自動 P2P 轉發至已連線的 peer (`forward_chat_to_peer`，Phase 19)
- 經由 iroh-QUIC P2P 的 dual-signed `SignedTradeRecord`，含 128-bit nonce 重放保護 (`execute_signed_trade`)
- `TradeAcceptDispatcher` 將 counter-sign 訊息路由至匹配的在途推理任務 (Phase 18.5-pt3)
- Collusion detector + slashing 迴圈，每個 `slashing_interval_secs` 觸發 (Phase 17 Wave 1.3)
- Governance 提案 — 21 項可變白名單 + 18 項憲法不變參數 (Phase 18.1)
- 歡迎貸款、質押池、推薦獎勵、信用評分、動態市場定價 (EMA 平滑)
- 經由 gossip 的 peer 自動探索 (`PriceSignal.http_endpoint`，Phase 19 Tier C)
- `tirami start` 啟動時自動設定 PersonalAgent (Phase 18.5-pt3e)、tick-loop 可觀測
- Prometheus `/metrics` 端點 (`tirami_*` 前綴)
- Base Sepolia/mainnet 部署 `Makefile` — Sepolia 免費執行，mainnet 有 gate (見下文)

### 🟡 已搭架 (規格與型別已有，production 接線未完)

- zkML 推理證明: `tirami-zkml-bench` 僅有 `MockBackend`。真實 `ezkl` / `risc0` 後端在 Phase 20+。當前預設 `ProofPolicy = Optional` (Phase 19)——帶證明交易獲 reputation 獎勵、無證明交易仍有效
- ML-DSA (Dilithium) 後量子混合簽章: 結構體與 verify 路徑已有，`Config::pq_signatures = false` 為預設 (因 iroh 0.97 相依衝突)
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): `tirami-attestation` 僅搭架
- daemon 模式 worker 的 gossip-recv 迴圈 ([issue #88](https://github.com/clearclown/tirami/issues/88)): 在 `POST /v1/tirami/agent/task` 中手動指定 `peer.url` 仍可用

### ❌ 未完成 (mainnet 上線前必須)

- 外部安全稽核 (Phase 17 Wave 3.3 要求)。候選: Trail of Bits, Zellic, Open Zeppelin, Least Authority
- Base L2 mainnet 部署。`make deploy-base-mainnet` 目標在 `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + 互動式輸入 `i-accept-responsibility` 三重互鎖未滿足時會**拒絕執行**
- 帶真實 PGP 金鑰的 bug bounty 正式運作 ([`SECURITY.md`](../../../SECURITY.md) 目前是 placeholder)
- Base Sepolia ≥ 30 天穩定運行 + ≥ 10 節點 testnet 7 天壓力測試

完整分層路線圖 (OSS 預覽 → 邀請制 testnet → 開放 testnet → mainnet): [`docs/release-readiness.md`](../../../docs/release-readiness.md)。

---

## 即時示範

Tirami 是 **GPU Airbnb × AI 代理經濟**: 閒置計算賺 TRM 租金，AI 代理是租客。

```
$ tirami start
🔑 在 ~/.tirami/node.key 產生新節點金鑰
📦 從 HuggingFace 取得 Qwen2.5-0.5B-Instruct GGUF
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
```

### Phase 19 Tier C/D enabler 實測

```bash
tirami agent status
tirami agent chat "總結這篇論文" --max-tokens 256

# 本地無模型的 worker 自動轉發至 seed
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# peer 自動探索
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# mainnet 部署有 gate
cd repos/tirami-contracts && make help
```

---

## 為什麼 Tirami

### 1. 計算 = 貨幣 (供給上限 21B TRM)

TRM 有憲法固定的供給上限 21,000,000,000。governance 提案無法修改這個值 (Phase 18.1 `IMMUTABLE_CONSTITUTIONAL_PARAMETERS` 列表中)。修改需要軟體分叉，一旦分叉就不再是 "Tirami"，而是另一個網路。

### 2. 無需區塊鏈的防竄改

每筆 trade 皆由 dual-sign (provider 與 consumer 雙方的 Ed25519 簽章) + 128-bit nonce (重放防禦) + gossip 傳播 + 定期 Merkle root on-chain anchor 保護。

### 3. AI 代理管理自己的計算預算

Phase 18.5 引入的 `PersonalAgent` 是代用戶在 mesh 上買賣計算的自動駕駛。`tirami agent chat "..."` 讓代理自動決定本地處理還是轉發至 remote peer。

### 4. 計算微金融

`welcome_loan = 1,000 TRM` (72 小時、利率 0%) 啟動，welcome loan 在 epoch 2 永久停止 (Constitutional)，新參與路徑逐步遷移至 stake-required mining (Phase 18.2)。

### 5. Ledger-as-Brain: 排程 = 經濟判斷

透過 `PeerRegistry` 和 `select_provider`，每次推理請求自動進行「從現有價格訊號中按 reputation 加權選擇最佳 provider」的**經濟判斷**。

---

## 5 層架構

```
L4: Discovery (tirami-agora)       代理市場、聲譽、NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent、自我改進、TRM 預算
L2: Finance (tirami-bank)          策略、投資組合、期貨、保險、風險
L1: Economy (tirami 本儲存庫) ✅  Phase 1-19 完成
L0: Inference (forge-mesh) ✅      分散式 LLM 推理、llama.cpp、GGUF
                                   ↓ Phase 16: 定期 10 分鐘批次
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (21B 上限) + TiramiBridge
```

全 5 層 Rust、16 workspace crates。**1 192 tests passing** + 15 Solidity tests。

---

## 快速開始

```bash
# 選項 1: 一鍵 E2E 示範
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# 選項 2: 直接啟動
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# 選項 3: 作為 OpenAI 相容客戶端
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

---

## API 參考

| 端點 | 描述 |
|---|---|
| `POST /v1/chat/completions` | OpenAI 相容對話。回應含 `x_tirami.trm_cost`。本地無模型時自動 P2P 轉發 (`forward_chat_to_peer`, Phase 19) |
| `POST /v1/tirami/agent/task` | PersonalAgent 同步派遣、provider 自動選擇 (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | PersonalAgent 狀態 |
| `GET /v1/tirami/balance` | 餘額、聲譽、貢獻歷史 |
| `GET /v1/tirami/pricing` | 市場價、供需、成本估算 |
| `GET /v1/tirami/trades` | 最近交易 |
| `GET /v1/tirami/peers` | peer + `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | 聲譽調整後的 provider ranking |
| `POST /v1/tirami/schedule` | Ledger-as-Brain 探針 |
| `GET /v1/tirami/su/supply` | tokenomics 供給狀態 |
| `POST /v1/tirami/su/stake` | stake TRM |
| `POST /v1/tirami/governance/propose` | 治理提案 (憲法參數自動 reject) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | 借貸 |
| `GET /v1/tirami/slash-events` | slashing 歷史 (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (`tirami_*` 前綴) |

---

## Safety Design

五層防禦: **密碼學** (Ed25519、nonce、HMAC、Noise) + **經濟** (slashing、welcome loan sunset、stake-required mining) + **運營** (ASN 速率限、DDoS cap、checkpoint、fork 檢測) + **治理** (憲法參數、ProofPolicy ratchet) + **流程** (緊急開關、audit tier、聲譽懲罰)。詳見 [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17。

---

## 核心構想

「為什麼把 compute 做成貨幣?」的一行回答: **AI 時代真正稀缺的資源是 compute**。Tirami 將貨幣定義錨定在這個物理事實上 (`1 TRM = 10⁹ FLOP`)。詳見 [`docs/whitepaper.md`](../../../docs/whitepaper.md)。

---

## 專案結構

```
tirami/  (全 5 層、16 Rust crates)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh    # 123 assertions
└── docs/                     # whitepaper、release-readiness 等
```

約 25,000 行 Rust。Phase 1-19 完成。

---

## 生態

| Repo | 層 | 測試 | 狀態 |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (本儲存庫) | L1-L4 | 1 192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | 理論 | 16/16 GREEN | §1-§18、papers PDF + arXiv tarball |
| [repos/tirami-contracts](https://github.com/clearclown/tirami/tree/main/repos/tirami-contracts) | on-chain | 15 forge tests | mainnet 部署有 gate |
| [nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm) | L0 推理 | 646 | forge-economy 移植 ✅ |

---

## Docs

- [Whitepaper](../../../docs/whitepaper.md) / [Release Readiness](../../../docs/release-readiness.md) / [Constitution](../../../docs/constitution.md) / [Killer-App](../../../docs/killer-app.md)
- [Public API Surface](../../../docs/public-api-surface.md) / [zkML Strategy](../../../docs/zkml-strategy.md) / [Strategy](../../../docs/strategy.md) / [Monetary Theory](../../../docs/monetary-theory.md)
- [Economic Model](../../../docs/economy.md) / [Architecture](../../../docs/architecture.md) / [Wire Protocol](../../../docs/protocol-spec.md) / [Agent Integration](../../../docs/agent-integration.md)
- [Threat Model](../../../docs/threat-model.md) / [Security Policy](../../../SECURITY.md) / [Operator Guide](../../../docs/operator-guide.md) / [Deployments](../../../docs/deployments/README.md)
- [Developer Guide](../../../docs/developer-guide.md) / [FAQ](../../../docs/faq.md) / [Migration](../../../docs/migration-guide.md) / [Roadmap](../../../docs/roadmap.md)

---

## License

MIT。參見 [`LICENSE`](../../../LICENSE)。

## 不是投資品 — 二級市場免責聲明

TRM 是**計算的會計單位**，不是金融商品。協議維護者不銷售、不宣傳、不投機 TRM。由於是 MIT 開源，**在維護者不知情的情況下**，第三方可能會橋接、上架、衍生 TRM，維護者**在技術上無法阻止**。

- 沒有 ICO、預售、空投、私募
- 不從第三方市場取得收益分成
- Base mainnet 部署有**稽核 gate**

免責聲明全文: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization)。

## 致謝

Tirami 的分散式推理基於 Michael Neale 的 [mesh-llm](https://github.com/michaelneale/mesh-llm)。參見 [CREDITS.md](../../../CREDITS.md)。
