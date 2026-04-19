<div align="center">

# Tirami

**计算即货币。每一瓦特都在产生智能，而非浪费。**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · **简体中文** · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · [Français](../fr/README.md) · [Русский](../ru/README.md) · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> 权威版本为英文 [`README.md`](../../../README.md)。译文可能存在延迟。

</div>

**Tirami 是一个计算即货币的分布式推理协议。** 节点通过为他人执行有用的 LLM 推理来赚取 TRM (Tirami Resource Merit)。与 Bitcoin 烧电做无意义哈希运算不同，Tirami 节点消耗的每一焦耳都在产生某人真正需要的智能。

分布式推理引擎基于 Michael Neale 的 [mesh-llm](https://github.com/michaelneale/mesh-llm)。Tirami 在其上加入了计算经济——TRM 会计、Proof of Useful Work、动态定价、自主代理预算、失效安全控制。参见 [CREDITS.md](../../../CREDITS.md)。

**集成分叉：** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — 内嵌 Tirami 经济层的 mesh-llm。

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

在讨论其他任何事之前，首先明确**现在能用**和**还不能用**的部分。Tirami 是 MIT 许可的开源软件，**不是代币销售**。没有 ICO，没有预挖，没有团队金库，没有空投。TRM 是计算的会计单位 (1 TRM = 10⁹ FLOP)，不是金融产品——参见 [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization)。

### ✅ 当前可用 (Rust 1 192 测试 + Solidity 15 测试，已验证)

- OpenAI 兼容 HTTP 聊天 + 自动 P2P 转发到已连接的 peer (`forward_chat_to_peer`，Phase 19)
- 经由 iroh-QUIC P2P 的 dual-signed `SignedTradeRecord`，含 128-bit nonce 重放保护 (`execute_signed_trade`)
- `TradeAcceptDispatcher` 将 counter-sign 消息路由到匹配的在途推理任务 (Phase 18.5-pt3)
- Collusion detector + slashing 循环，每个 `slashing_interval_secs` 触发 (Phase 17 Wave 1.3)
- Governance 提案 — 21 项可变白名单 + 18 项宪法不变参数 (Phase 18.1)
- 欢迎贷款、质押池、推荐奖励、信用评分、动态市场定价 (EMA 平滑)
- 经由 gossip 的 peer 自动发现 (`PriceSignal.http_endpoint`，Phase 19 Tier C)
- `tirami start` 启动时自动配置 PersonalAgent (Phase 18.5-pt3e)，tick-loop 可观测
- Prometheus `/metrics` 端点 (`tirami_*` 前缀)
- Base Sepolia/mainnet 部署 `Makefile` — Sepolia 免费执行，mainnet 有 gate (见下文)

### 🟡 已搭架 (规格与类型已有，production 接线未完)

- zkML 推理证明: `tirami-zkml-bench` 仅有 `MockBackend`。真实 `ezkl` / `risc0` 后端在 Phase 20+。当前默认 `ProofPolicy = Optional` (Phase 19)——带证明交易获 reputation 奖励、无证明交易仍有效
- ML-DSA (Dilithium) 后量子混合签名: 结构体与 verify 路径已有，`Config::pq_signatures = false` 为默认 (因 iroh 0.97 依赖冲突)
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): `tirami-attestation` 仅搭架
- daemon 模式 worker 的 gossip-recv 循环 ([issue #88](https://github.com/clearclown/tirami/issues/88)): 在 `POST /v1/tirami/agent/task` 中手动指定 `peer.url` 仍可用

### ❌ 未完成 (mainnet 上线前必须)

- 外部安全审计 (Phase 17 Wave 3.3 要求)。候选: Trail of Bits, Zellic, Open Zeppelin, Least Authority
- Base L2 mainnet 部署。`make deploy-base-mainnet` 目标在 `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + 交互式输入 `i-accept-responsibility` 三重互锁未满足时会**拒绝执行**
- 带真实 PGP 密钥的 bug bounty 正式运行 ([`SECURITY.md`](../../../SECURITY.md) 目前是 placeholder)
- Base Sepolia ≥ 30 天稳定运行 + ≥ 10 节点 testnet 7 天 stress test

完整分层路线图 (OSS 预览 → 邀请制 testnet → 开放 testnet → mainnet): [`docs/release-readiness.md`](../../../docs/release-readiness.md)。

---

## 实时演示

Tirami 是 **GPU Airbnb × AI 代理经济**: 闲置计算赚 TRM 租金，AI 代理是租客。

```
$ tirami start
🔑 在 ~/.tirami/node.key 生成新节点密钥
📦 从 HuggingFace 获取 Qwen2.5-0.5B-Instruct GGUF
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
```

### Phase 19 Tier C/D enabler 实测

```bash
tirami agent status
tirami agent chat "总结这篇论文" --max-tokens 256

# 本地无模型的 worker 自动转发到 seed
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# peer 自动发现
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# mainnet 部署有 gate
cd repos/tirami-contracts && make help
```

---

## 为什么 Tirami

### 1. 计算 = 货币 (供给上限 21B TRM)

TRM 有宪法固定的供给上限 21,000,000,000。governance 提案无法修改这个值 (Phase 18.1 `IMMUTABLE_CONSTITUTIONAL_PARAMETERS` 列表中)。修改需要软件分叉，一旦分叉就不再是 "Tirami"，而是另一个网络。

### 2. 无需区块链的防篡改

每笔 trade 都由 dual-sign (provider 与 consumer 双方的 Ed25519 签名) + 128-bit nonce (重放防御) + gossip 传播 + 定期 Merkle root on-chain anchor 保护。

### 3. AI 代理管理自己的计算预算

Phase 18.5 引入的 `PersonalAgent` 是代用户在 mesh 上买卖计算的自动驾驶。`tirami agent chat "..."` 让代理自动决定本地处理还是转发到 remote peer。

### 4. 计算微金融

`welcome_loan = 1,000 TRM` (72 小时，利率 0%) 启动，welcome loan 在 epoch 2 永久停止 (Constitutional)，新参与路径逐步迁移到 stake-required mining (Phase 18.2)。

### 5. Ledger-as-Brain: 调度 = 经济判断

通过 `PeerRegistry` 和 `select_provider`，每次推理请求自动进行"从现有价格信号中按 reputation 加权选择最优 provider"的**经济判断**。

---

## 5 层架构

```
L4: Discovery (tirami-agora)       代理市场、声誉、NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent、自我改进、TRM 预算
L2: Finance (tirami-bank)          策略、投资组合、期货、保险、风险
L1: Economy (tirami 本仓库) ✅     Phase 1-19 完成
L0: Inference (forge-mesh) ✅      分布式 LLM 推理、llama.cpp、GGUF
                                   ↓ Phase 16: 定期 10 分钟批次
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (21B 上限) + TiramiBridge
```

全 5 层 Rust，16 workspace crates。**1 192 tests passing** + 15 Solidity tests。

---

## 快速开始

```bash
# 选项 1: 一键 E2E 演示
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# 选项 2: 直接启动
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# 选项 3: 作为 OpenAI 兼容客户端
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

---

## API 参考

| 端点 | 描述 |
|---|---|
| `POST /v1/chat/completions` | OpenAI 兼容聊天。响应含 `x_tirami.trm_cost`。本地无模型时自动 P2P 转发 (`forward_chat_to_peer`, Phase 19) |
| `POST /v1/tirami/agent/task` | PersonalAgent 同步调度、provider 自动选择 (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | PersonalAgent 状态 |
| `GET /v1/tirami/balance` | 余额、声誉、贡献历史 |
| `GET /v1/tirami/pricing` | 市场价、供需、成本估算 |
| `GET /v1/tirami/trades` | 最近交易 |
| `GET /v1/tirami/peers` | peer + `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | 声誉调整后的 provider ranking |
| `POST /v1/tirami/schedule` | Ledger-as-Brain 探针 |
| `GET /v1/tirami/su/supply` | tokenomics 供给状态 |
| `POST /v1/tirami/su/stake` | stake TRM |
| `POST /v1/tirami/governance/propose` | 治理提案 (宪法参数自动 reject) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | 借贷 |
| `GET /v1/tirami/slash-events` | slashing 历史 (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (`tirami_*` 前缀) |

---

## Safety Design

五层防御: **密码学** (Ed25519、nonce、HMAC、Noise) + **经济** (slashing、welcome loan sunset、stake-required mining) + **运营** (ASN 速率限、DDoS cap、checkpoint、fork 检测) + **治理** (宪法参数、ProofPolicy ratchet) + **流程** (紧急开关、audit tier、声誉惩罚)。详见 [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17。

---

## 核心想法

"为什么把 compute 做成货币?"的一行回答: **AI 时代真正稀缺的资源是 compute**。Tirami 将货币定义锚定在这个物理事实上 (`1 TRM = 10⁹ FLOP`)。详见 [`docs/whitepaper.md`](../../../docs/whitepaper.md)。

---

## 项目结构

```
tirami/  (全 5 层、16 Rust crates)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh    # 123 assertions
└── docs/                     # whitepaper、release-readiness 等
```

约 25,000 行 Rust。Phase 1-19 完成。

---

## 生态

| Repo | 层 | 测试 | 状态 |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (本仓库) | L1-L4 | 1 192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | 理论 | 16/16 GREEN | §1-§18、papers PDF + arXiv tarball |
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

MIT。参见 [`LICENSE`](../../../LICENSE)。

## 不是投资品 — 二级市场免责声明

TRM 是**计算的会计单位**，不是金融产品。协议维护者不销售、不宣传、不投机 TRM。由于是 MIT 开源，**在维护者不知情的情况下**，第三方可能会桥接、上架、派生 TRM，维护者**在技术上无法阻止**。

- 没有 ICO、预售、空投、私募
- 不从第三方市场获得收益分成
- Base mainnet 部署有**审计 gate**

免责声明全文: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization)。

## 致谢

Tirami 的分布式推理基于 Michael Neale 的 [mesh-llm](https://github.com/michaelneale/mesh-llm)。参见 [CREDITS.md](../../../CREDITS.md)。
