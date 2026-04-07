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
