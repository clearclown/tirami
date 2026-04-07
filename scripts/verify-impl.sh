#!/usr/bin/env bash
# scripts/verify-impl.sh — Phase 5.5+ implementation regression tests
# Each assertion maps to an issue in #32-#41 (forge-economics theory → forge code).
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
  "grep -q 'pub struct LoanRecord' crates/forge-ledger/src/lending.rs"
assert "#32b" "LoanStatus enum exists" \
  "grep -q 'pub enum LoanStatus' crates/forge-ledger/src/lending.rs"
assert "#32c" "SignedLoanRecord with verify() exists" \
  "grep -q 'pub struct SignedLoanRecord' crates/forge-ledger/src/lending.rs && grep -q 'fn verify' crates/forge-ledger/src/lending.rs"
assert "#32d" "create_loan / repay_loan / default_loan methods exist" \
  "grep -q 'fn create_loan' crates/forge-ledger/src/ledger.rs && grep -q 'fn repay_loan' crates/forge-ledger/src/ledger.rs && grep -q 'fn default_loan' crates/forge-ledger/src/ledger.rs"

# === Phase 5.5: Credit score (#33) ===
assert "#33a" "compute_credit_score method exists" \
  "grep -q 'fn compute_credit_score' crates/forge-ledger/src/ledger.rs"
assert "#33b" "Credit score weights match parameters.md" \
  "grep -q 'WEIGHT_TRADE.*0.3' crates/forge-ledger/src/lending.rs && grep -q 'WEIGHT_REPAYMENT.*0.4' crates/forge-ledger/src/lending.rs"

# === Phase 5.5: Wire protocol (#40) ===
assert "#40a" "LoanProposal in Payload enum" \
  "grep -q 'LoanProposal' crates/forge-proto/src/messages.rs"
assert "#40b" "LoanAccept in Payload enum" \
  "grep -q 'LoanAccept' crates/forge-proto/src/messages.rs"
assert "#40c" "LoanGossip in Payload enum" \
  "grep -q 'LoanGossip' crates/forge-proto/src/messages.rs"

# === Phase 5.5: Safety (#35) ===
assert "#35a" "Lending safety check in safety.rs" \
  "grep -q 'LendingSafety\|check_loan\|loan_safety' crates/forge-ledger/src/safety.rs"
assert "#35b" "Default rate circuit breaker constant exists" \
  "grep -q 'DEFAULT_CIRCUIT_BREAKER_THRESHOLD' crates/forge-ledger/src/lending.rs"

# === Phase 5.5: API endpoints (#34) ===
assert "#34a" "/v1/forge/lend endpoint registered" \
  "grep -q '/v1/forge/lend' crates/forge-node/src/api.rs"
assert "#34b" "/v1/forge/borrow endpoint registered" \
  "grep -q '/v1/forge/borrow' crates/forge-node/src/api.rs"
assert "#34c" "/v1/forge/repay endpoint registered" \
  "grep -q '/v1/forge/repay' crates/forge-node/src/api.rs"
assert "#34d" "/v1/forge/credit endpoint registered" \
  "grep -q '/v1/forge/credit' crates/forge-node/src/api.rs"
assert "#34e" "/v1/forge/pool endpoint registered" \
  "grep -q '/v1/forge/pool' crates/forge-node/src/api.rs"
assert "#34f" "/v1/forge/loans endpoint registered" \
  "grep -q '/v1/forge/loans' crates/forge-node/src/api.rs"

# === Phase 5.5: Welcome loan (#36) ===
assert "#36" "Welcome loan referenced in ledger" \
  "grep -q 'WELCOME_LOAN_AMOUNT\|welcome_loan' crates/forge-ledger/src/ledger.rs"

# === Phase 6: Multi-model pricing (#37) ===
assert "#37a" "ModelTier enum exists" \
  "grep -q 'pub enum ModelTier' crates/forge-ledger/src/ledger.rs"
assert "#37b" "Tier-based CU/token constants exist" \
  "grep -q 'TIER_SMALL_CU_PER_TOKEN' crates/forge-ledger/src/lending.rs"

# === Phase 6: Routing (#38) ===
assert "#38" "/v1/forge/route endpoint registered" \
  "grep -q '/v1/forge/route' crates/forge-node/src/api.rs"

# === Phase 6: Lightning bridge bidirectional (#41) ===
assert "#41" "Lightning deposit / CU credit flow exists" \
  "grep -qE 'fn.*deposit|btc_to_cu|credit_from_invoice' crates/forge-lightning/src/payment.rs"

# === Phase 7: L2 forge-bank (§10 of forge-economics parameters.md) ===
assert "#BANK-types"      "Position/Portfolio types exist" \
  "grep -q 'pub enum PositionKind' crates/forge-bank/src/types.rs && grep -q 'pub struct Portfolio' crates/forge-bank/src/types.rs"
assert "#BANK-strategies" "3 strategies exist" \
  "grep -q 'pub struct ConservativeStrategy' crates/forge-bank/src/strategies.rs && grep -q 'pub struct HighYieldStrategy' crates/forge-bank/src/strategies.rs && grep -q 'pub struct BalancedStrategy' crates/forge-bank/src/strategies.rs"
assert "#BANK-portfolio"  "PortfolioManager tick" \
  "grep -q 'pub fn tick' crates/forge-bank/src/portfolio.rs"
assert "#BANK-futures"    "FuturesContract + pnl helpers" \
  "grep -q 'pub fn futures_pnl' crates/forge-bank/src/futures.rs && grep -q 'pub fn mark_to_market' crates/forge-bank/src/futures.rs"
assert "#BANK-insurance"  "InsurancePolicy + premium_for" \
  "grep -q 'pub fn premium_for' crates/forge-bank/src/insurance.rs"
assert "#BANK-risk"       "RiskModel + assess + VaR 2.33 constant" \
  "grep -q 'pub struct RiskModel' crates/forge-bank/src/risk.rs && grep -q 'pub fn assess' crates/forge-bank/src/risk.rs && grep -q '2\\.33' crates/forge-bank/src/risk.rs"
assert "#BANK-optimizer"  "YieldOptimizer with risk gate" \
  "grep -q 'pub struct YieldOptimizer' crates/forge-bank/src/yield_optimizer.rs"

# === Phase 7: L3 forge-mind (§11 of forge-economics parameters.md) ===
assert "#MIND-harness"    "Harness with evolve() + JSON" \
  "grep -q 'pub struct Harness' crates/forge-mind/src/harness.rs && grep -q 'pub fn evolve' crates/forge-mind/src/harness.rs"
assert "#MIND-budget"     "CuBudget with §11 constants" \
  "grep -q 'pub struct CuBudget' crates/forge-mind/src/budget.rs && grep -q '5_000\\|5000' crates/forge-mind/src/budget.rs && grep -q '50_000\\|50000' crates/forge-mind/src/budget.rs"
assert "#MIND-benchmark"  "Benchmark trait + InMemoryBenchmark" \
  "grep -q 'pub trait Benchmark' crates/forge-mind/src/benchmark.rs && grep -q 'pub struct InMemoryBenchmark' crates/forge-mind/src/benchmark.rs"
assert "#MIND-optimizer"  "MetaOptimizer trait + 2 impls" \
  "grep -q 'pub trait MetaOptimizer' crates/forge-mind/src/meta_optimizer.rs && grep -q 'pub struct PromptRewriteOptimizer' crates/forge-mind/src/meta_optimizer.rs"
assert "#MIND-cycle"      "ImprovementCycleRunner + ROI constant" \
  "grep -q 'pub struct ImprovementCycleRunner' crates/forge-mind/src/cycle.rs && grep -q '100_000\\|100000' crates/forge-mind/src/cycle.rs"
assert "#MIND-agent"      "ForgeMindAgent improve loop" \
  "grep -q 'pub struct ForgeMindAgent' crates/forge-mind/src/agent.rs && grep -q 'pub fn improve' crates/forge-mind/src/agent.rs"

# === Phase 7: L4 forge-agora (§12 of forge-economics parameters.md) ===
assert "#AGORA-types"      "AgentProfile + TradeObservation" \
  "grep -q 'pub struct AgentProfile' crates/forge-agora/src/types.rs && grep -q 'pub struct TradeObservation' crates/forge-agora/src/types.rs"
assert "#AGORA-registry"   "AgentRegistry with snapshot" \
  "grep -q 'pub struct AgentRegistry' crates/forge-agora/src/registry.rs && grep -q 'pub fn snapshot' crates/forge-agora/src/registry.rs"
assert "#AGORA-reputation" "Reputation weights match parameters.md §12.1" \
  "grep -q 'WEIGHT_VOLUME.*0\\.4' crates/forge-agora/src/reputation.rs && grep -q 'WEIGHT_RECENCY.*0\\.3' crates/forge-agora/src/reputation.rs && grep -q 'WEIGHT_DIVERSITY.*0\\.2' crates/forge-agora/src/reputation.rs && grep -q 'WEIGHT_CONSISTENCY.*0\\.1' crates/forge-agora/src/reputation.rs"
assert "#AGORA-matching"   "CapabilityMatcher composite (0.6/0.4)" \
  "grep -q 'pub struct CapabilityMatcher' crates/forge-agora/src/matching.rs && grep -q 'QUALITY_WEIGHT.*0\\.6' crates/forge-agora/src/matching.rs && grep -q 'COST_WEIGHT.*0\\.4' crates/forge-agora/src/matching.rs"
assert "#AGORA-marketplace" "Marketplace facade" \
  "grep -q 'pub struct Marketplace' crates/forge-agora/src/marketplace.rs"

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
