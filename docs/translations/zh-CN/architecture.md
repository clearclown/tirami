# Forge — 架构设计

## 概述

Forge 是一个双层系统：**推理层**和**经济层**。

推理层处理模型分发、网格网络和 API 服务。它基于 [mesh-llm](https://github.com/Mesh-LLM/mesh-llm) 构建。

经济层处理 CU 核算、交易记录、定价和代理预算。这是 Forge 的原创贡献。

```
┌─────────────────────────────────────────────────┐
│  SDK / 集成边界                                 │
│  任何客户端都可以将 forge-node 作为库嵌入          │
│  第三方代理、仪表板、适配器                       │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  经济层 (Forge 原创)                             │
│                                                  │
│  ┌──────────────┐ ┌──────────┐ ┌─────────────┐ │
│  │ forge-ledger │ │ pricing  │ │ agent       │ │
│  │ CU 交易      │ │ 供需关系  │ │ 预算管理    │ │
│  │ 声誉         │ │          │ │ /v1/forge/* │ │
│  │ 收益         │ │          │ │             │ │
│  └──────────────┘ └──────────┘ └─────────────┘ │
│                                                  │
│  ┌──────────────┐ ┌──────────────────────────┐  │
│  │ forge-verify │ │ forge-bridge (可选)       │  │
│  │ 双重签名      │ │ CU ↔ BTC Lightning      │  │
│  │ gossip 同步   │ │ CU ↔ 稳定币              │  │
│  └──────────────┘ └──────────────────────────┘  │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  推理层 (源自 mesh-llm)                          │
│                                                  │
│  ┌────────────┐ ┌───────────┐ ┌──────────────┐ │
│  │ iroh 网格  │ │ llama.cpp │ │ OpenAI API   │ │
│  │ QUIC+Noise │ │ 流水线并行  │ │ /v1/chat/    │ │
│  │ Nostr 发现 │ │ MoE 分片   │ │ completions  │ │
│  └────────────┘ └───────────┘ └──────────────┘ │
└─────────────────────────────────────────────────┘
```

## 推理层 (mesh-llm)

推理层负责：

- **网格网络**: 基于 iroh 的 QUIC 连接，使用 Noise 加密
- **节点发现**: 公共网格使用 Nostr 中继，局域网使用 mDNS
- **模型分发**: 稠密模型使用流水线并行，MoE 模型使用专家分片
- **推理执行**: 通过 llama-server 和 rpc-server 子进程运行 llama.cpp
- **API 服务**: 兼容 OpenAI 的 `/v1/chat/completions` 和 `/v1/models`

Forge 从 mesh-llm 继承了所有这些功能。推理层不知道 CU、交易或定价。

## 经济层 (Forge)

经济层位于推理层之上，负责：

### forge-ledger — 经济引擎

```rust
pub struct ComputeLedger {
    balances: HashMap<NodeId, NodeBalance>,
    work_log: Vec<WorkUnit>,
    trade_log: Vec<TradeRecord>,
    price: MarketPrice,
}
```

核心职责：
- 跟踪每个节点的 CU 余额（贡献、消耗、预留）
- 记录每次推理交易（提供者、消费者、CU 金额、token 数）
- 根据供需关系计算动态市场价格
- 向贡献节点发放收益
- 为协议外桥接导出结算报表
- 使用 HMAC-SHA256 完整性保护将快照持久化到磁盘

### forge-verify — 有用工作证明 (目标)

确保 CU 声明是合法的：
- 双重签名协议：提供者和消费者都对每个 TradeRecord 进行签名
- Gossip 同步：签名的交易在网络中传播
- 验证：任何节点都可以验证双方签名
- 欺诈检测：拒绝不匹配或未签名的交易

### forge-bridge — 外部结算 (可选)

为需要它的操作员将 CU 转换为外部价值：
- 比特币 Lightning: 按可配置的汇率将 CU → msats
- 稳定币: 通过适配器将 CU → USDC/USDT
- 法定货币: 通过操作员仪表板将 CU → 银行转账

桥接层位于核心协议之外。不同的操作员可以使用不同的桥接。

### API 接口

| 路由 | 层级 | 描述 |
|-------|-------|-------------|
| `POST /v1/chat/completions` | 推理 + 经济 | 运行推理，记录 CU 交易 |
| `GET /v1/models` | 推理 | 列出已加载的模型 |
| `GET /v1/forge/balance` | 经济 | CU 余额、声誉 |
| `GET /v1/forge/pricing` | 经济 | 市场价格、成本估算 |
| `GET /status` | 经济 | 市场价格、网络统计、最近交易 |
| `GET /topology` | 推理 | 模型清单、节点、分片计划 |
| `GET /settlement` | 经济 | 可导出的交易历史 |
| `GET /health` | 推理 | 基本健康检查 |

## 数据流

### 带有 CU 核算的推理

```
消费者发送请求
    ↓
API 接收 POST /v1/chat/completions
    ↓
账本检查: can_afford(consumer, estimated_cost)?
    ↓ 是
推理层执行 (llama-server / rpc-server)
    ↓
Token 流回消费者
    ↓
账本记录交易:
  - provider.contributed += cu_cost
  - consumer.consumed += cu_cost
  - trade_log.push(TradeRecord)
    ↓
响应包含 x_forge: { cu_cost, effective_balance }
```

### 结算导出

```
操作员运行: forge settle --hours 24
    ↓
API 读取该时间窗口的 trade_log
    ↓
按节点汇总: 总收益、总支出、净 CU
    ↓
导出带有可选参考价格的 JSON 报表
    ↓
操作员使用桥接适配器将净 CU 转换为 BTC/法币
```

## 安全模型

```
第 0 层: 比特币主链        ← 可选锚定 (未来)
第 1 层: 双重签名          ← 提供者 + 消费者为每笔交易签名
第 2 层: HMAC-SHA256 账本  ← 本地完整性保护
第 3 层: iroh (QUIC+Noise) ← 传输加密
第 4 层: 推理执行          ← 模型在提供者本地运行
```

每一层都防御不同的威胁：
- 第 4 层: 模型完整性 (GGUF 哈希验证)
- 第 3 层: 传输机密性 (窃听)
- 第 2 层: 本地篡改 (文件修改)
- 第 1 层: 网络欺诈 (虚假 CU 声明)
- 第 0 层: 历史不可篡改性 (可选比特币锚定)

## Crate 依赖关系

```
forge-core ← 共享类型 (NodeId, CU, Config)
    ↑
forge-ledger ← 经济引擎 (交易、定价、收益)
    ↑
forge-lightning ← 外部桥接 (LDK 钱包, CU↔sats)
    ↑
forge-node ← 协调器 (HTTP API, 流水线, 账本集成)
    ↑
forge-cli ← 参考 CLI (聊天、播种、工人、结算)

forge-net ← P2P 传输 (iroh, QUIC, Noise, mDNS)
forge-proto ← 线缆消息 (bincode, 14 种负载类型)
forge-infer ← 推理引擎 (llama.cpp, GGUF 加载器)
forge-shard ← 拓扑规划 (层级分配、重新平衡)
```
