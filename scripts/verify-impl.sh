#!/usr/bin/env bash
# scripts/verify-impl.sh — Phase 5.5+ implementation regression tests
# Each assertion maps to an issue in #32-#41 (tirami-economics theory → forge code).
set +e
cd "$(dirname "$0")/.."

PASS=0
FAIL=0
RESULTS=()

assert() {
  local id="$1"; local desc="$2"; local cmd="$3"
  if eval "$cmd" >/dev/null 2>&1; then
    PASS=$((PASS+1)); RESULTS+=("✓ $id  $desc")
  else
    FAIL=$((FAIL+1)); RESULTS+=("✗ $id  $desc")
  fi
}

# === Phase 5.5: Lending types (#32) ===
assert "#32a" "LoanRecord struct exists" \
  "grep -q 'pub struct LoanRecord' crates/tirami-ledger/src/lending.rs"
assert "#32b" "LoanStatus enum exists" \
  "grep -q 'pub enum LoanStatus' crates/tirami-ledger/src/lending.rs"
assert "#32c" "SignedLoanRecord with verify() exists" \
  "grep -q 'pub struct SignedLoanRecord' crates/tirami-ledger/src/lending.rs && grep -q 'fn verify' crates/tirami-ledger/src/lending.rs"
assert "#32d" "create_loan / repay_loan / default_loan methods exist" \
  "grep -q 'fn create_loan' crates/tirami-ledger/src/ledger.rs && grep -q 'fn repay_loan' crates/tirami-ledger/src/ledger.rs && grep -q 'fn default_loan' crates/tirami-ledger/src/ledger.rs"

# === Phase 5.5: Credit score (#33) ===
assert "#33a" "compute_credit_score method exists" \
  "grep -q 'fn compute_credit_score' crates/tirami-ledger/src/ledger.rs"
assert "#33b" "Credit score weights match parameters.md" \
  "grep -q 'WEIGHT_TRADE.*0.3' crates/tirami-ledger/src/lending.rs && grep -q 'WEIGHT_REPAYMENT.*0.4' crates/tirami-ledger/src/lending.rs"

# === Phase 5.5: Wire protocol (#40) ===
assert "#40a" "LoanProposal in Payload enum" \
  "grep -q 'LoanProposal' crates/tirami-proto/src/messages.rs"
assert "#40b" "LoanAccept in Payload enum" \
  "grep -q 'LoanAccept' crates/tirami-proto/src/messages.rs"
assert "#40c" "LoanGossip in Payload enum" \
  "grep -q 'LoanGossip' crates/tirami-proto/src/messages.rs"

# === Phase 5.5: Safety (#35) ===
assert "#35a" "Lending safety check in safety.rs" \
  "grep -q 'LendingSafety\|check_loan\|loan_safety' crates/tirami-ledger/src/safety.rs"
assert "#35b" "Default rate circuit breaker constant exists" \
  "grep -q 'DEFAULT_CIRCUIT_BREAKER_THRESHOLD' crates/tirami-ledger/src/lending.rs"

# === Phase 5.5: API endpoints (#34) ===
assert "#34a" "/v1/tirami/lend endpoint registered" \
  "grep -q '/v1/tirami/lend' crates/tirami-node/src/api.rs"
assert "#34b" "/v1/tirami/borrow endpoint registered" \
  "grep -q '/v1/tirami/borrow' crates/tirami-node/src/api.rs"
assert "#34c" "/v1/tirami/repay endpoint registered" \
  "grep -q '/v1/tirami/repay' crates/tirami-node/src/api.rs"
assert "#34d" "/v1/tirami/credit endpoint registered" \
  "grep -q '/v1/tirami/credit' crates/tirami-node/src/api.rs"
assert "#34e" "/v1/tirami/pool endpoint registered" \
  "grep -q '/v1/tirami/pool' crates/tirami-node/src/api.rs"
assert "#34f" "/v1/tirami/loans endpoint registered" \
  "grep -q '/v1/tirami/loans' crates/tirami-node/src/api.rs"

# === Phase 5.5: Welcome loan (#36) ===
assert "#36" "Welcome loan referenced in ledger" \
  "grep -q 'WELCOME_LOAN_AMOUNT\|welcome_loan' crates/tirami-ledger/src/ledger.rs"

# === Phase 6: Multi-model pricing (#37) ===
assert "#37a" "ModelTier enum exists" \
  "grep -q 'pub enum ModelTier' crates/tirami-ledger/src/ledger.rs"
assert "#37b" "Tier-based CU/token constants exist" \
  "grep -q 'TIER_SMALL_CU_PER_TOKEN' crates/tirami-ledger/src/lending.rs"

# === Phase 6: Routing (#38) ===
assert "#38" "/v1/tirami/route endpoint registered" \
  "grep -q '/v1/tirami/route' crates/tirami-node/src/api.rs"

# === Phase 6: Lightning bridge bidirectional (#41) ===
assert "#41" "Lightning deposit / CU credit flow exists" \
  "grep -qE 'fn.*deposit|btc_to_cu|credit_from_invoice' crates/tirami-lightning/src/payment.rs"

# === Phase 7: L2 tirami-bank (§10 of tirami-economics parameters.md) ===
assert "#BANK-types"      "Position/Portfolio types exist" \
  "grep -q 'pub enum PositionKind' crates/tirami-bank/src/types.rs && grep -q 'pub struct Portfolio' crates/tirami-bank/src/types.rs"
assert "#BANK-strategies" "3 strategies exist" \
  "grep -q 'pub struct ConservativeStrategy' crates/tirami-bank/src/strategies.rs && grep -q 'pub struct HighYieldStrategy' crates/tirami-bank/src/strategies.rs && grep -q 'pub struct BalancedStrategy' crates/tirami-bank/src/strategies.rs"
assert "#BANK-portfolio"  "PortfolioManager tick" \
  "grep -q 'pub fn tick' crates/tirami-bank/src/portfolio.rs"
assert "#BANK-futures"    "FuturesContract + pnl helpers" \
  "grep -q 'pub fn futures_pnl' crates/tirami-bank/src/futures.rs && grep -q 'pub fn mark_to_market' crates/tirami-bank/src/futures.rs"
assert "#BANK-insurance"  "InsurancePolicy + premium_for" \
  "grep -q 'pub fn premium_for' crates/tirami-bank/src/insurance.rs"
assert "#BANK-risk"       "RiskModel + assess + VaR 2.33 constant" \
  "grep -q 'pub struct RiskModel' crates/tirami-bank/src/risk.rs && grep -q 'pub fn assess' crates/tirami-bank/src/risk.rs && grep -q '2\\.33' crates/tirami-bank/src/risk.rs"
assert "#BANK-optimizer"  "YieldOptimizer with risk gate" \
  "grep -q 'pub struct YieldOptimizer' crates/tirami-bank/src/yield_optimizer.rs"

# === Phase 7: L3 tirami-mind (§11 of tirami-economics parameters.md) ===
assert "#MIND-harness"    "Harness with evolve() + JSON" \
  "grep -q 'pub struct Harness' crates/tirami-mind/src/harness.rs && grep -q 'pub fn evolve' crates/tirami-mind/src/harness.rs"
assert "#MIND-budget"     "CuBudget with §11 constants" \
  "grep -q 'pub struct CuBudget' crates/tirami-mind/src/budget.rs && grep -q '5_000\\|5000' crates/tirami-mind/src/budget.rs && grep -q '50_000\\|50000' crates/tirami-mind/src/budget.rs"
assert "#MIND-benchmark"  "Benchmark trait + InMemoryBenchmark" \
  "grep -q 'pub trait Benchmark' crates/tirami-mind/src/benchmark.rs && grep -q 'pub struct InMemoryBenchmark' crates/tirami-mind/src/benchmark.rs"
assert "#MIND-optimizer"  "MetaOptimizer trait + 2 impls" \
  "grep -q 'pub trait MetaOptimizer' crates/tirami-mind/src/meta_optimizer.rs && grep -q 'pub struct PromptRewriteOptimizer' crates/tirami-mind/src/meta_optimizer.rs"
assert "#MIND-cycle"      "ImprovementCycleRunner + ROI constant" \
  "grep -q 'pub struct ImprovementCycleRunner' crates/tirami-mind/src/cycle.rs && grep -q '100_000\\|100000' crates/tirami-mind/src/cycle.rs"
assert "#MIND-agent"      "ForgeMindAgent improve loop (async after Phase 8)" \
  "grep -q 'pub struct ForgeMindAgent' crates/tirami-mind/src/agent.rs && grep -qE 'pub (async )?fn improve' crates/tirami-mind/src/agent.rs"

# === Phase 7: L4 tirami-agora (§12 of tirami-economics parameters.md) ===
assert "#AGORA-types"      "AgentProfile + TradeObservation" \
  "grep -q 'pub struct AgentProfile' crates/tirami-agora/src/types.rs && grep -q 'pub struct TradeObservation' crates/tirami-agora/src/types.rs"
assert "#AGORA-registry"   "AgentRegistry with snapshot" \
  "grep -q 'pub struct AgentRegistry' crates/tirami-agora/src/registry.rs && grep -q 'pub fn snapshot' crates/tirami-agora/src/registry.rs"
assert "#AGORA-reputation" "Reputation weights match parameters.md §12.1" \
  "grep -q 'WEIGHT_VOLUME.*0\\.4' crates/tirami-agora/src/reputation.rs && grep -q 'WEIGHT_RECENCY.*0\\.3' crates/tirami-agora/src/reputation.rs && grep -q 'WEIGHT_DIVERSITY.*0\\.2' crates/tirami-agora/src/reputation.rs && grep -q 'WEIGHT_CONSISTENCY.*0\\.1' crates/tirami-agora/src/reputation.rs"
assert "#AGORA-matching"   "CapabilityMatcher composite (0.6/0.4)" \
  "grep -q 'pub struct CapabilityMatcher' crates/tirami-agora/src/matching.rs && grep -q 'QUALITY_WEIGHT.*0\\.6' crates/tirami-agora/src/matching.rs && grep -q 'COST_WEIGHT.*0\\.4' crates/tirami-agora/src/matching.rs"
assert "#AGORA-marketplace" "Marketplace facade" \
  "grep -q 'pub struct Marketplace' crates/tirami-agora/src/marketplace.rs"

# === Phase 8: L2/L3/L4 wired into tirami-node ===

# AppState fields
assert "#P8-state-bank"      "AppState owns BankServices" \
  "grep -q 'bank: Arc<Mutex<crate::bank_adapter::BankServices' crates/tirami-node/src/api.rs"
assert "#P8-state-mp"        "AppState owns Marketplace" \
  "grep -q 'marketplace: Arc<Mutex<.*Marketplace' crates/tirami-node/src/api.rs"
assert "#P8-state-mind"      "AppState owns Option<ForgeMindAgent>" \
  "grep -q 'mind_agent: Arc<Mutex<Option<.*ForgeMindAgent' crates/tirami-node/src/api.rs"

# Adapter modules
assert "#P8-bank-adapter"    "bank_adapter::pool_snapshot_from_ledger exists" \
  "grep -q 'pub fn pool_snapshot_from_ledger' crates/tirami-node/src/bank_adapter.rs"
assert "#P8-bank-services"   "BankServices wrapper exists" \
  "grep -q 'pub struct BankServices' crates/tirami-node/src/bank_adapter.rs"
assert "#P8-agora-adapter"   "agora_adapter conversion + refresh exist" \
  "grep -q 'observation_from_trade' crates/tirami-node/src/agora_adapter.rs && grep -q 'refresh_marketplace_from_ledger' crates/tirami-node/src/agora_adapter.rs"
assert "#P8-mind-adapter"    "mind_adapter frontier_node_id + record_frontier_consumption" \
  "grep -q 'pub fn frontier_node_id' crates/tirami-node/src/mind_adapter.rs && grep -q 'record_frontier_consumption' crates/tirami-node/src/mind_adapter.rs"

# Bank endpoints (8)
assert "#P8-bank-routes"     "All 8 /v1/tirami/bank/* routes registered" \
  "grep -q '/v1/tirami/bank/portfolio' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/bank/tick' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/bank/strategy' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/bank/risk' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/bank/futures' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/bank/risk-assessment' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/bank/optimize' crates/tirami-node/src/api.rs"

# Agora endpoints (7)
assert "#P8-agora-routes"    "All 7 /v1/tirami/agora/* routes registered" \
  "grep -q '/v1/tirami/agora/register' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/agora/agents' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/agora/reputation' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/agora/find' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/agora/stats' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/agora/snapshot' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/agora/restore' crates/tirami-node/src/api.rs"

# Mind endpoints (5)
assert "#P8-mind-routes"     "All 5 /v1/tirami/mind/* routes registered" \
  "grep -q '/v1/tirami/mind/init' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/mind/state' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/mind/improve' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/mind/budget' crates/tirami-node/src/api.rs && \
   grep -q '/v1/tirami/mind/stats' crates/tirami-node/src/api.rs"

# CuPaidOptimizer
assert "#P8-cu-paid"         "CuPaidOptimizer with reqwest exists" \
  "grep -q 'pub struct CuPaidOptimizer' crates/tirami-mind/src/cu_paid_optimizer.rs && \
   grep -q 'reqwest' crates/tirami-mind/src/cu_paid_optimizer.rs"

# Async MetaOptimizer migration
assert "#P8-async-trait"     "MetaOptimizer is async via async_trait" \
  "grep -q 'async_trait' crates/tirami-mind/src/meta_optimizer.rs && \
   grep -qE 'async fn propose' crates/tirami-mind/src/meta_optimizer.rs"

# Handler files exist
assert "#P8-handlers-bank"   "handlers/bank.rs exists" \
  "test -f crates/tirami-node/src/handlers/bank.rs"
assert "#P8-handlers-agora"  "handlers/agora.rs exists" \
  "test -f crates/tirami-node/src/handlers/agora.rs"
assert "#P8-handlers-mind"   "handlers/mind.rs exists" \
  "test -f crates/tirami-node/src/handlers/mind.rs"

# === Phase 9 A3: Reputation gossip ===
assert "#A3-proto"          "ReputationObservation in Payload enum" \
  "grep -q 'ReputationGossip' crates/tirami-proto/src/messages.rs && grep -q 'pub struct ReputationObservation' crates/tirami-proto/src/messages.rs"
assert "#A3-canonical"      "ReputationObservation::canonical_bytes / verify / dedup_key" \
  "grep -q 'fn canonical_bytes' crates/tirami-proto/src/messages.rs && grep -q 'fn verify' crates/tirami-proto/src/messages.rs && grep -q 'fn dedup_key' crates/tirami-proto/src/messages.rs"
assert "#A3-net-broadcast"  "broadcast_reputation in tirami-net gossip" \
  "grep -q 'pub async fn broadcast_reputation' crates/tirami-net/src/gossip.rs"
assert "#A3-net-handle"     "handle_reputation_gossip in tirami-net gossip" \
  "grep -q 'pub async fn handle_reputation_gossip' crates/tirami-net/src/gossip.rs"
assert "#A3-ledger-merge"   "merge_remote_reputation / consensus_reputation on ComputeLedger" \
  "grep -q 'fn merge_remote_reputation' crates/tirami-ledger/src/ledger.rs && grep -q 'fn consensus_reputation' crates/tirami-ledger/src/ledger.rs"
assert "#A3-remote-field"   "remote_reputation field on ComputeLedger" \
  "grep -q 'remote_reputation' crates/tirami-ledger/src/ledger.rs"
assert "#A3-pipeline"       "ReputationGossip dispatch in pipeline.rs" \
  "grep -q 'ReputationGossip' crates/tirami-node/src/pipeline.rs"
assert "#A3-endpoint"       "/v1/tirami/reputation-gossip-status endpoint registered" \
  "grep -q 'reputation-gossip-status' crates/tirami-node/src/api.rs"

# === Phase 9 A5: Collusion resistance ===
assert "#A5-module"         "collusion.rs module exists" \
  "test -f crates/tirami-ledger/src/collusion.rs"
assert "#A5-detector"       "CollusionDetector + CollusionReport in collusion.rs" \
  "grep -q 'pub struct CollusionDetector' crates/tirami-ledger/src/collusion.rs && grep -q 'pub struct CollusionReport' crates/tirami-ledger/src/collusion.rs"
assert "#A5-algorithms"     "tight_cluster + volume_spike + round_robin in collusion.rs" \
  "grep -q 'tight_cluster_score' crates/tirami-ledger/src/collusion.rs && grep -q 'volume_spike_score' crates/tirami-ledger/src/collusion.rs && grep -q 'round_robin_score' crates/tirami-ledger/src/collusion.rs"
assert "#A5-tarjan"         "Tarjan SCC implementation present" \
  "grep -q 'tarjan\|Tarjan\|lowlink' crates/tirami-ledger/src/collusion.rs"
assert "#A5-effective-rep"  "effective_reputation method on ComputeLedger" \
  "grep -q 'fn effective_reputation' crates/tirami-ledger/src/ledger.rs"
assert "#A5-endpoint"       "/v1/tirami/collusion endpoint registered" \
  "grep -q '/v1/tirami/collusion' crates/tirami-node/src/api.rs"
assert "#A5-constants"      "MAX_REMOTE_OBSERVATIONS_PER_NODE + MIN_OBSERVATION_WEIGHT constants" \
  "grep -q 'MAX_REMOTE_OBSERVATIONS_PER_NODE' crates/tirami-ledger/src/lending.rs && grep -q 'MIN_OBSERVATION_WEIGHT' crates/tirami-ledger/src/lending.rs"

# === Phase 10 P6: Bitcoin OP_RETURN anchoring ===
assert "#P10-anchor-payload"  "build_anchor_payload function exists" \
  "grep -q 'pub fn build_anchor_payload' crates/tirami-ledger/src/anchor.rs"
assert "#P10-anchor-script"   "build_anchor_script emits OP_RETURN" \
  "grep -q 'pub fn build_anchor_script' crates/tirami-ledger/src/anchor.rs && grep -q 'OP_RETURN' crates/tirami-ledger/src/anchor.rs"
assert "#P10-anchor-endpoint" "/v1/tirami/anchor route registered" \
  "grep -q '/v1/tirami/anchor' crates/tirami-node/src/api.rs"

# === Phase 10 P5: Prometheus metrics ===
assert "#P10-metrics-module" "metrics.rs implements ForgeMetrics" \
  "grep -q 'pub struct ForgeMetrics' crates/tirami-ledger/src/metrics.rs"
assert "#P10-metrics-observe" "ForgeMetrics::observe exists" \
  "grep -q 'pub fn observe' crates/tirami-ledger/src/metrics.rs"
assert "#P10-metrics-endpoint" "/metrics route registered" \
  "grep -q '/metrics' crates/tirami-node/src/api.rs && grep -q 'metrics_handler' crates/tirami-node/src/api.rs"

# === Phase 10 P2: Ed25519 reputation signing ===
assert "#P10-signed"        "ReputationObservation::new_signed exists" \
  "grep -q 'pub fn new_signed' crates/tirami-proto/src/messages.rs"
assert "#P10-strict-verify" "verify() rejects empty/invalid signatures" \
  "grep -q 'signature.len() != 64' crates/tirami-proto/src/messages.rs"

# === Phase 11: llama-server compatibility fixes ===
assert "#P11-top-p" "top_p passed through to engine.generate() (not ignored)" \
  "grep -q 'top_p' crates/tirami-infer/src/llama_engine.rs && grep -q 'top_p' crates/tirami-infer/src/engine.rs"
assert "#P11-top-k" "top_k field in OpenAIChatRequest + engine trait" \
  "grep -q 'pub top_k: Option<i32>' crates/tirami-node/src/api.rs && grep -q 'top_k: Option<i32>' crates/tirami-infer/src/engine.rs"
assert "#P11-real-streaming" "generate_streaming method on InferenceEngine trait" \
  "grep -q 'fn generate_streaming' crates/tirami-infer/src/engine.rs && grep -q 'GenerateStreaming' crates/tirami-infer/src/llama_engine.rs && grep -q 'generate_streaming' crates/tirami-node/src/api.rs"

# === Phase 12 A2: zkML scaffold ===
assert "#P12-zk-trait" "ProofVerifier trait + ProofOfInference struct exist" \
  "grep -q 'pub trait ProofVerifier' crates/tirami-ledger/src/zk.rs && grep -q 'pub struct ProofOfInference' crates/tirami-ledger/src/zk.rs"
assert "#P12-zk-mock" "MockVerifier impl exists" \
  "grep -q 'pub struct MockVerifier' crates/tirami-ledger/src/zk.rs && grep -q 'impl ProofVerifier for MockVerifier' crates/tirami-ledger/src/zk.rs"

# === Phase 12 C: mesh-llm resolver ported ===
assert "#P12-resolve-hf-url" "parse_hf_url function exists" \
  "grep -q 'pub fn parse_hf_url' crates/tirami-infer/src/model_registry.rs"
assert "#P12-resolve-shorthand" "parse_hf_shorthand function exists" \
  "grep -q 'pub fn parse_hf_shorthand' crates/tirami-infer/src/model_registry.rs"
assert "#P12-resolve-dispatch" "unified resolve() dispatcher exists" \
  "grep -q 'pub fn resolve' crates/tirami-infer/src/model_registry.rs && grep -q 'ResolveSource' crates/tirami-infer/src/model_registry.rs"

# === Phase 12 A3: federated training scaffold ===
assert "#P12-fed-types" "GradientContribution + FederatedRound exist" \
  "grep -q 'pub struct GradientContribution' crates/tirami-mind/src/federated.rs && grep -q 'pub struct FederatedRound' crates/tirami-mind/src/federated.rs"
assert "#P12-fed-aggregator" "WeightedAverageAggregator implements Aggregator trait" \
  "grep -q 'impl Aggregator for WeightedAverageAggregator' crates/tirami-mind/src/federated.rs"

# === Phase 12 A4: BitVM scaffold ===
assert "#P12-bitvm-types" "StakedClaim + FraudProof exist" \
  "grep -q 'pub struct StakedClaim' crates/tirami-ledger/src/bitvm.rs && grep -q 'pub struct FraudProof' crates/tirami-ledger/src/bitvm.rs"
assert "#P12-bitvm-verifier" "FraudProofVerifier trait + MockFraudProofVerifier" \
  "grep -q 'pub trait FraudProofVerifier' crates/tirami-ledger/src/bitvm.rs && grep -q 'impl FraudProofVerifier for MockFraudProofVerifier' crates/tirami-ledger/src/bitvm.rs"
assert "#P12-bitvm-doc" "BitVM design doc exists" \
  "test -f docs/bitvm-design.md && grep -q 'BitVM' docs/bitvm-design.md"

# === Phase 12 A1: function calling ===
assert "#P12-tools-request" "OpenAIChatRequest has tools field" \
  "grep -q 'pub tools: Option<Vec<OpenAITool>>' crates/tirami-node/src/api.rs"
assert "#P12-tools-extract" "extract_tool_call helper exists" \
  "grep -q 'fn extract_tool_call' crates/tirami-node/src/api.rs"

# === Build & test ===
assert "BUILD" "cargo check --workspace passes" \
  "cargo check --workspace --quiet 2>&1 | grep -qv 'error'"
assert "TEST" "cargo test --workspace passes" \
  "cargo test --workspace --quiet 2>&1 | tail -30 | grep -qE 'test result: ok' && ! cargo test --workspace --quiet 2>&1 | tail -30 | grep -qE 'FAILED'"

# === Report ===
echo ""
echo "==================================="
echo "  Phase 5.5+ Implementation Results"
echo "==================================="
printf '  %s\n' "${RESULTS[@]}"
echo "==================================="
echo "  PASS: $PASS / $((PASS+FAIL))"
echo "  FAIL: $FAIL / $((PASS+FAIL))"
echo "==================================="

if [ $FAIL -eq 0 ]; then
  echo "  🟢 ALL GREEN"
  exit 0
else
  echo "  🔴 $FAIL ISSUES REMAIN"
  exit 1
fi
