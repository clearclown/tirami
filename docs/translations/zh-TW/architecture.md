# Forge — 架構設計

## 概述

Forge 是一個雙層系統：**推理層**和**經濟層**。

推理層處理模型分發、網格網絡和 API 服務。它基於 [mesh-llm](https://github.com/Mesh-LLM/mesh-llm) 建構。

經濟層處理 CU 核算、交易記錄、定價和代理預算。這是 Forge 的原創貢獻。

```
┌─────────────────────────────────────────────────┐
│  SDK / 集成邊界                                 │
│  任何客戶端都可以將 forge-node 作為庫嵌入          │
│  第三方代理、儀表板、適配器                       │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  經濟層 (Forge 原創)                             │
│                                                  │
│  ┌──────────────┐ ┌──────────┐ ┌─────────────┐ │
│  │ forge-ledger │ │ pricing  │ │ agent       │ │
│  │ CU 交易      │ │ 供需關係  │ │ 預算管理    │ │
│  │ 聲譽         │ │          │ │ /v1/forge/* │ │
│  │ 收益         │ │          │ │             │ │
│  └──────────────┘ └──────────┘ └─────────────┘ │
│                                                  │
│  ┌──────────────┐ ┌──────────────────────────┐  │
│  │ forge-verify │ │ forge-bridge (可選)       │  │
│  │ 雙重簽名      │ │ CU ↔ BTC Lightning      │  │
│  │ gossip 同步   │ │ CU ↔ 穩定幣              │  │
│  └──────────────┘ └──────────────────────────┘  │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  推理層 (源自 mesh-llm)                          │
│                                                  │
│  ┌────────────┐ ┌───────────┐ ┌──────────────┐ │
│  │ iroh 網格  │ │ llama.cpp │ │ OpenAI API   │ │
│  │ QUIC+Noise │ │ 流水線並行  │ │ /v1/chat/    │ │
│  │ Nostr 發現 │ │ MoE 分片   │ │ completions  │ │
│  └────────────┘ └───────────┘ └──────────────┘ │
└─────────────────────────────────────────────────┘
```

## 推理層 (mesh-llm)

推理層負責：

- **網格網絡**: 基於 iroh 的 QUIC 連接，使用 Noise 加密
- **節點發現**: 公共網格使用 Nostr 中繼，區域網使用 mDNS
- **模型分發**: 稠密模型使用流水線並行，MoE 模型使用專家分片
- **推理執行**: 透過 llama-server 和 rpc-server 子進程運行 llama.cpp
- **API 服務**: 相容 OpenAI 的 `/v1/chat/completions` 和 `/v1/models`

Forge 從 mesh-llm 繼承了所有這些功能。推理層不知道 CU、交易或定價。

## 經濟層 (Forge)

經濟層位於推理層之上，負責：

### forge-ledger — 經濟引擎

```rust
pub struct ComputeLedger {
    balances: HashMap<NodeId, NodeBalance>,
    work_log: Vec<WorkUnit>,
    trade_log: Vec<TradeRecord>,
    price: MarketPrice,
}
```

核心職責：
- 跟蹤每個節點的 CU 餘額（貢獻、消耗、預留）
- 記錄每次推理交易（提供者、消費者、CU 金額、token 數）
- 根據供需關係計算動態市場價格
- 向貢獻節點發放收益
- 為協定外橋接導出結算報表
- 使用 HMAC-SHA256 完整性保護將快照持久化到磁盤

### forge-verify — 有用工作證明 (目標)

確保 CU 聲明是合法的：
- 雙重簽名協定：提供者和消費者都對每個 TradeRecord 進行簽名
- Gossip 同步：簽名的交易在網絡中傳播
- 驗證：任何節點都可以驗證雙方簽名
- 欺詐檢測：拒絕不匹配或未簽名的交易

### forge-bridge — 外部結算 (可選)

為需要它的操作員將 CU 轉換為外部價值：
- 比特幣 Lightning: 按可配置的匯率將 CU → msats
- 穩定幣: 透過適配器將 CU → USDC/USDT
- 法定貨幣: 透過操作員儀表板將 CU → 銀行轉帳

橋接層位於核心協定之外。不同的操作員可以使用不同的橋接。

### API 接口

| 路由 | 層級 | 描述 |
|-------|-------|-------------|
| `POST /v1/chat/completions` | 推理 + 經濟 | 運行推理，記錄 CU 交易 |
| `GET /v1/models` | 推理 | 列出已加載的模型 |
| `GET /v1/forge/balance` | 經濟 | CU 餘額、聲譽 |
| `GET /v1/forge/pricing` | 經濟 | 市場價格、成本估算 |
| `GET /status` | 經濟 | 市場價格、網絡統計、最近交易 |
| `GET /topology` | 推理 | 模型清單、節點、分片計劃 |
| `GET /settlement` | 經濟 | 可導出的交易歷史 |
| `GET /health` | 推理 | 基本健康檢查 |

## 數據流

### 帶有 CU 核算的推理

```
消費者發送請求
    ↓
API 接收 POST /v1/chat/completions
    ↓
帳本檢查: can_afford(consumer, estimated_cost)?
    ↓ 是
推理層執行 (llama-server / rpc-server)
    ↓
Token 流回消費者
    ↓
帳本記錄交易:
  - provider.contributed += cu_cost
  - consumer.consumed += cu_cost
  - trade_log.push(TradeRecord)
    ↓
回應包含 x_forge: { cu_cost, effective_balance }
```

### 結算導出

```
操作員運行: forge settle --hours 24
    ↓
API 讀取該時間窗口的 trade_log
    ↓
按節點匯總: 總收益、總支出、淨 CU
    ↓
導出帶有可選參考價格的 JSON 報表
    ↓
操作員使用橋接適配器將淨 CU 轉換為 BTC/法幣
```

## 安全模型

```
第 0 層: 比特幣主鏈        ← 可選錨定 (未來)
第 1 層: 雙重簽名          ← 提供者 + 消費者為每筆交易簽名
第 2 層: HMAC-SHA256 帳本  ← 本地完整性保護
第 3 層: iroh (QUIC+Noise) ← 傳輸加密
第 4 層: 推理執行          ← 模型在提供者本地運行
```

每一層都防禦不同的威脅：
- 第 4 層: 模型完整性 (GGUF 哈希驗證)
- 第 3 層: 傳輸機密性 (竊聽)
- 第 2 層: 本地篡改 (文件修改)
- 第 1 層: 網絡欺詐 (虛假 CU 聲明)
- 第 0 層: 歷史不可篡改性 (可選比特幣錨定)

## Crate 依賴關係

```
forge-core ← 共享類型 (NodeId, CU, Config)
    ↑
forge-ledger ← 經濟引擎 (交易、定價、收益)
    ↑
forge-lightning ← 外部橋接 (LDK 錢包, CU↔sats)
    ↑
forge-node ← 協調器 (HTTP API, 流水線, 帳本集成)
    ↑
forge-cli ← 參考 CLI (聊天、播種、工人、結算)

forge-net ← P2P 傳輸 (iroh, QUIC, Noise, mDNS)
forge-proto ← 線纜消息 (bincode, 14 種負載類型)
forge-infer ← 推理引擎 (llama.cpp, GGUF 加載器)
forge-shard ← 拓撲規劃 (層級分配、重新平衡)
```
