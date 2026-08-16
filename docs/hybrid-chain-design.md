# Tirami ハイブリッドチェーン実装設計書

> Phase 16 以降の on-chain 連携の実装指針。理論的根拠は
> [tirami-economics/docs/15-hybrid-chain.md](https://github.com/clearclown/tirami-economics/blob/main/docs/15-hybrid-chain.md) を参照。

**ステータス:** 設計フェーズ (コードなし、Phase 16 着手時に実装開始)。
**対象読者:** tirami コアコントリビューター。

---

## 1. 背景

### 1.1 なぜハイブリッドか

v1 の "No blockchain in core" 原則は**推論のホットパス**では正しいが、以下の課題を解決できない:

- **レジャーの分岐**: 100 台超では gossip + 双方署名だけでは eventual consistency が取れない
- **TRM の外部価値**: 人間が BTC/USDC で TRM を買えない (エージェントへの予算注入経路なし)
- **紛争解決**: 相反する取引が gossip された場合、どちらが正か決定不能

「**投機されるならしょうがない**」というユーザー判断を踏まえ、TRM の on-chain 表現を正面から認める。ただし推論レイテンシを損なわないため、ブロック確認待ちは avoid する。

### 1.2 設計原則

1. **ホットパス = off-chain**: 推論取引は現行通り、ゴシップ + ローカルレジャー + 双方署名
2. **コールドパス = on-chain**: 10 分ごとのバッチ決済、DEX での TRM 流通
3. **Single source of truth**: 同一ノードで off-chain 残高と on-chain 残高が乖離する余地をゼロにする (always-2-way bridge)
4. **プロトコル変更を最小化**: 既存の `TradeRecord` / `SignedTradeRecord` / Merkle root 計算は維持。新設するのは bridge コントラクトとバッチャーのみ

---

## 2. アーキテクチャ

```
   ┌───────────────────────── HOT PATH ────────────────────────┐
   │                                                           │
   │   Consumer      Provider      Provider      Consumer      │
   │      │             │             │             │          │
   │      │ inference   │ inference   │ inference   │          │
   │      └──► ledger ──┘─► ledger ◄──┘─► ledger ◄──┘          │
   │             │             │             │                 │
   │             └──────── gossip ───────────┘                 │
   │             (TradeGossip, PriceSignalGossip)              │
   │                                                           │
   └────────┬──────────────────────────────────────────────────┘
            │  every 10 min
            ▼
   ┌───────────────────────── BATCHER ─────────────────────────┐
   │   Each node independently computes:                       │
   │     merkle_root = ComputeLedger::compute_trade_merkle_root │
   │     net_balances_delta = Σ (this-batch trades per node)    │
   │     pouw_claims        = Σ (verified FLOP per provider)    │
   └────────┬──────────────────────────────────────────────────┘
            │
            ▼
   ┌───────────────────────── COLD PATH ───────────────────────┐
   │                                                           │
   │   Tirami Bridge Contract (Base L2)                        │
   │   ┌───────────────────────────────────────────┐           │
   │   │  storeBatch(root, batch_id, node_id)      │           │
   │   │  mintForProvider(node_id, to, flops, ...) │           │
   │   │  withdraw(off_chain_balance_proof)        │           │
   │   │  deposit(erc20_transfer_event)            │           │
   │   └───────────────────────────────────────────┘           │
   │                                                           │
   │   TRM ERC-20 Token (Base L2)                              │
   │   ┌───────────────────────────────────────────┐           │
   │   │  totalSupply <= 21,000,000,000 × 10^18    │           │
   │   │  mint(address, amount) — bridge only      │           │
   │   │  burn(amount) — self-service slash        │           │
   │   │  Uniswap V4 pair: TRM/USDC                │           │
   │   └───────────────────────────────────────────┘           │
   │                                                           │
   └───────────────────────────────────────────────────────────┘
```

---

## 3. チェーン選定: Base L2

候補比較 (tirami-economics/docs/15-hybrid-chain.md §15.3 参照):

| 項目 | Base L2 | Solana | Cosmos appchain |
|------|---------|--------|----------------|
| TPS | ~1,000 | ~65,000 | カスタム |
| ガス代 (10分バッチ) | ~$0.001 | ~$0.00025 | 独自 |
| ERC-20 エコシステム | ✅ | SPL | ❌ |
| ファイナリティ | ~2秒 | ~400ms | カスタム |
| TRM 流通 | Uniswap 即対応 | Jupiter | 独自 DEX |

**Base L2 採用理由:**
- Ethereum のセキュリティを継承
- Uniswap v4 で自動 LP 形成 (TRM/USDC ペア)
- OP Stack のオープン規格 → 別 L2 への移植が容易
- Coinbase のユーザベースから自然な流入

**不採用理由:**
- Solana: 10 分バッチ用途に TPS 過剰、Rust 依存チェーン間コンフリクトリスク
- Cosmos: 運用コスト高、IBC で別 chain に飛ばしにくい

---

## 4. スマートコントラクト仕様

### 4.1 TRM ERC-20 トークン

```solidity
contract TRM is ERC20, Ownable {
    uint256 public constant TOTAL_SUPPLY_CAP = 21_000_000_000 * 10**18;
    address public bridge;

    modifier onlyBridge() {
        require(msg.sender == bridge, "only bridge");
        _;
    }

    function mint(address to, uint256 amount) external onlyBridge {
        require(totalSupply() + amount <= TOTAL_SUPPLY_CAP, "supply cap");
        _mint(to, amount);
    }

    function burn(uint256 amount) external {
        _burn(msg.sender, amount);
    }
}
```

**不変条件:**
- `totalSupply` は `TOTAL_SUPPLY_CAP` を超えない
- `mint` は Bridge コントラクトからのみ
- `burn` は任意 (staking slash 実装用)

### 4.2 Bridge コントラクト

```solidity
contract TiramiBridge {
    TRM public immutable trm;
    mapping(bytes32 => bool) public consumedBatches;
    mapping(bytes32 => uint64) public lastMintForNode;

    uint256 public constant MINT_COOLDOWN = 10 minutes;
    uint256 public constant WITHDRAWAL_DELAY = 60 minutes;

    event BatchStored(bytes32 indexed merkleRoot, uint64 batchId, bytes32 indexed nodeId);
    event MintForProvider(bytes32 indexed nodeId, uint256 flops, uint256 minted);
    event Deposit(address indexed user, bytes32 indexed nodeId, uint256 amount);
    event WithdrawalRequested(bytes32 indexed nodeId, address indexed to, uint256 amount, uint256 unlockAt);
    event WithdrawalClaimed(bytes32 indexed nodeId, address indexed to, uint256 amount);

    /// 10 分バッチの Merkle root を記録。validator のみ、batchId で dedup。
    function storeBatch(
        bytes32 merkleRoot,
        uint64 batchId,
        bytes32 nodeId
    ) external;

    /// 保存済み batch root に対する PoUW proof を検証して新規 TRM を mint。
    function mintForProvider(
        bytes32 nodeId,
        address to,
        uint256 flops,
        uint64 batchId,
        bytes32[] calldata merkleProof
    ) external;

    /// on-chain TRM を off-chain credit に変換 (即時)。
    function deposit(bytes32 nodeId, uint256 amount) external;

    /// off-chain 残高の withdrawal 要求 (60 分遅延 + challenge period)。
    function requestWithdrawal(
        bytes32 nodeId,
        uint256 amount,
        bytes32 balanceRoot,
        bytes32[] calldata merkleProof
    ) external;

    /// 遅延期間経過後の引き出し実行。
    function claimWithdrawal(bytes32 nodeId) external;
}
```

---

## 5. Off-chain ↔ On-chain ブリッジフロー

### 5.1 Deposit: on-chain → off-chain

```
User                  Bridge Contract       Tirami Node
 │                          │                   │
 │ transfer TRM             │                   │
 │───────────────────────►  │                   │
 │                          │ emit Deposit      │
 │                          │───────────────────►
 │                          │                   │ credit off-chain balance
 │                          │                   │ (reservable within 1 block)
```

**実装責任**: 各 tirami-node が Base L2 の `Deposit` イベントを購読 → ローカルレジャーに即時反映。二重クレジット防止のため `event_hash` を dedup set に保持。

### 5.2 Withdrawal: off-chain → on-chain

```
User Agent           Tirami Node       Bridge Contract
    │                    │                   │
    │ withdraw request   │                   │
    │ (signed)           │                   │
    │───────────────────►│                   │
    │                    │ burn off-chain TRM
    │                    │ include in next batch
    │                    │ (10 min later)     │
    │                    │ storeBatch         │
    │                    │───────────────────►│
    │                    │                   │ merkle root stored
    │                    │                   │ (60 min challenge window)
    │                    │                   │
    │                    │ requestWithdrawal  │
    │                    │ (merkle proof)     │
    │                    │───────────────────►│
    │                    │                   │ schedule unlock
    │                    │                   │ (+ 60 min delay)
    │ claimWithdrawal    │                   │
    │──────────────────────────────────────► │
    │                    │                   │ transfer TRM
    │ ◄──────────────────────────────────────│
```

**challenge period の意味**: 60 分の間に他ノードが矛盾する Merkle root を提出した場合、fraud proof で差し戻し可能。これにより不正 withdrawal の経済的期待値がマイナスになる。

### 5.3 Mint: PoUW → 新規 TRM 発行

```
Provider Node                 Bridge
      │                          │
      │ run inference, earn TRM  │
      │ (off-chain)              │
      │                          │
      │ 10-min batch:            │
      │   Σ flops_estimated      │
      │   merkle proof           │
      │                          │
      │ mintForProvider(         │
      │   nodeId, to, flops,     │
      │   batchId, proof)        │
      │───────────────────────►  │
      │                          │ verify proof
      │                          │ mint TRM to provider's
      │                          │ associated wallet
```

**reward 計算**: `trm_minted = flops_estimated / 10⁹` (原理1 の厳密適用)。ただし supply cap に達している場合は mint 失敗 → fees のみ。

---

## 6. パラメータ

tirami-economics/spec/parameters.md §14 への追加予定:

| パラメータ | 値 | 説明 |
|----------|-----|------|
| `anchor_interval_minutes` | 10 | Merkle root アンカリング間隔 |
| `batch_max_trades` | 10,000 | 1 バッチの最大取引数 |
| `bridge_confirmation_blocks` | 12 | Deposit 確認に必要な L2 ブロック数 |
| `withdrawal_delay_minutes` | 60 | Off-chain → On-chain 出金の遅延 |
| `mint_cooldown_minutes` | 10 | 同一ノードの PoUW mint 最小間隔 |
| `bridge_chain_id` | 8453 | Base Mainnet chain ID |
| `trm_decimals` | 18 | ERC-20 標準 |

---

## 7. 実装ステップ (Phase 16 以降)

### Step A: バッチャー実装 (Rust)

**新 crate: `tirami-anchor`**

```rust
pub struct Anchorer {
    ledger: Arc<Mutex<ComputeLedger>>,
    chain_client: Arc<dyn ChainClient>,  // trait, Base/Solana/Mock implementations
    interval: Duration,
    batch_id_counter: AtomicU64,
}

impl Anchorer {
    pub async fn run(&self) -> anyhow::Result<()> {
        let mut ticker = interval(self.interval);
        loop {
            ticker.tick().await;
            self.anchor_batch().await?;
        }
    }

    async fn anchor_batch(&self) -> anyhow::Result<()> {
        let (root, deltas) = {
            let ledger = self.ledger.lock().await;
            (ledger.compute_trade_merkle_root(), ledger.compute_batch_deltas())
        };
        self.chain_client.store_batch(root, self.next_batch_id(), deltas).await
    }
}
```

### Step B: ChainClient trait

```rust
#[async_trait]
pub trait ChainClient: Send + Sync {
    async fn store_batch(&self, root: [u8; 32], batch_id: u64, deltas: BatchDeltas) -> Result<TxHash>;
    async fn mint_for_provider(&self, node: NodeId, to: Address, flops: u64, batch_id: u64, proof: MerkleProof) -> Result<TxHash>;
    async fn subscribe_deposits(&self) -> mpsc::Receiver<DepositEvent>;
    async fn request_withdrawal(&self, node: NodeId, amount: u64, proof: MerkleProof) -> Result<TxHash>;
}
```

実装候補:
- `BaseClient` (ethers-rs ベース、本番)
- `MockChainClient` (テスト用、インメモリ)
- `SolanaClient` (将来オプション)

### Step C: Solidity コントラクト (Foundry)

**新リポジトリ: `tirami-contracts`**

```
tirami-contracts/
├── src/
│   ├── TRM.sol
│   ├── TiramiBridge.sol
│   └── lib/MerkleProof.sol
├── test/
│   ├── TRM.t.sol
│   ├── TiramiBridge.t.sol
│   └── Integration.t.sol
└── script/Deploy.s.sol
```

### Step D: Daemon 統合

`TiramiNode::run_seed` に Anchorer を tokio::spawn で組み込み:

```rust
let anchorer = Anchorer::new(
    self.ledger.clone(),
    Arc::new(BaseClient::new(config.bridge_rpc_url.clone())),
    Duration::from_secs(config.anchor_interval_minutes * 60),
);
tokio::spawn(async move { anchorer.run().await });
```

---

## 8. セキュリティ分析

| 脅威 | 緩和策 |
|------|--------|
| 二重支出 (off-chain → on-chain) | 60 分 challenge period + Merkle proof 検証 |
| 不正 batch 提出 | storeBatch は署名付き必須、nodeId 一意性チェック |
| Deposit replay | `event_hash` を dedup set に保持 (L2 reorg 耐性) |
| PoUW mint 詐称 | AuditTier に基づく audit challenge (Phase 14.3) |
| Bridge コントラクト攻撃 | OpenZeppelin 標準、Pausable、audit 必須 |
| L2 停止 | off-chain は継続稼働、L2 復旧後に自動リキャッチアップ |

---

## 9. 未解決項目 (次のフェーズで検討)

1. **Merkle proof 効率化**: 10,000 取引/バッチで gas cost がいくらになるか要実測
2. **Cross-L2 bridging**: Base から Solana への TRM 移動 (LayerZero / Wormhole 検討)
3. **Lightning Network との併用**: 既存の tirami-lightning を補完経路として残すか
4. **PoUW proof 検証**: FLOP 申告の検証方式 (ZK 推論証明 vs audit tier 合意)
5. **governance による bridge 停止**: 緊急時の killswitch 仕様

---

## 10. 参考文献

- [tirami-economics/docs/15-hybrid-chain.md](https://github.com/clearclown/tirami-economics/blob/main/docs/15-hybrid-chain.md) — 経済原理
- [tirami-economics/docs/16-agent-economy.md](https://github.com/clearclown/tirami-economics/blob/main/docs/16-agent-economy.md) — エージェント経済圏
- [OP Stack](https://docs.optimism.io/) — Base L2 の基盤
- [OpenZeppelin Bridge patterns](https://docs.openzeppelin.com/) — コントラクト参考実装
- [Uniswap v4 hooks](https://blog.uniswap.org/uniswap-v4) — TRM/USDC pair 実装
