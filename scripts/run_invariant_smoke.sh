#!/usr/bin/env bash
# run_invariant_smoke.sh — blocking invariant smoke gate for critical safety-kernel tests.
# Fails if any smoke test fails OR if any filter matches zero tests (no silent no-op).

set -euo pipefail

# Each entry is "category:package:target_args:test_name".
# target_args is either "--lib" or "--test <target_name>".
TESTS=(
  "TTL enforcement (gateway HTTP):ferrum-integration-tests:--test integration_gateway_capabilities:test_gateway_rejects_capability_ttl_over_300"
  "TTL enforcement (store service):ferrum-gateway:--lib:capabilities::tests::test_ttl_301_rejected"
  "Single-use CAS/reuse (gateway HTTP):ferrum-integration-tests:--test integration_gateway_capabilities:test_single_use_capability_cannot_be_reused_via_gateway"
  "Single-use CAS/concurrent (store service):ferrum-gateway:--lib:capabilities::tests::test_mark_used_concurrent_single_use"
  "Minimum lineage chain:ferrum-integration-tests:--test integration_lineage_chain:test_lineage_chain_minimum_provenance_events"
  "Rollback/verify ordering:ferrum-integration-tests:--test integration_gateway_execution:test_verify_after_compensate_returns_409"
  "Provenance emission (approval):ferrum-integration-tests:--test integration_gateway_approvals:test_resolve_approval_provenance_event_emitted"
  "Provenance emission (policy bundle):ferrum-integration-tests:--test integration_gateway_bridges:test_policy_bundle_active_switch_emits_provenance"
  "R3 never auto-commit:ferrum-integration-tests:--test integration_gateway_execution:test_r3_contracts_have_auto_commit_false"
  "I11 output sanitization:ferrum-integration-tests:--test integration_gateway_execution:test_i11_sanitizes_execution_response_with_control_characters"
  "I5 scope constraints:ferrum-integration-tests:--test integration_gateway_outcomes:test_i5_scope_validation_resource_bindings_exceed_intent_scope"
)

total_passed=0
failed=0
zero_match=0

for entry in "${TESTS[@]}"; do
  IFS=':' read -r category package target_args test_name <<< "$entry"
  echo "==> $category ($package::$test_name)"

  set +e
  # shellcheck disable=SC2086
  output=$(cargo test -p "$package" $target_args "$test_name" -- --exact 2>&1)
  status=$?
  set -e

  # Find the libtest summary line.
  result_line=$(echo "$output" | grep -E '^test result: ' || true)

  if [[ -z "$result_line" ]]; then
    echo "    [ERROR] could not parse test result line for $category"
    failed=$((failed + 1))
    echo "$output"
    continue
  fi

  echo "    $result_line"

  # A zero-test match is a silent no-op; the gate must reject it.
  if echo "$result_line" | grep -qE '0 passed; 0 failed'; then
    echo "    [ERROR] zero tests matched for $category"
    zero_match=$((zero_match + 1))
    continue
  fi

  passed_count=$(echo "$result_line" | grep -oE '[0-9]+ passed' | grep -oE '[0-9]+' || echo 0)
  total_passed=$((total_passed + passed_count))

  if [[ $status -ne 0 ]]; then
    echo "    [ERROR] test failed for $category"
    failed=$((failed + 1))
    echo "$output"
    continue
  fi

done

echo "----------------------------------------"
echo "Invariant smoke total tests run: $total_passed"

if [[ $zero_match -gt 0 ]]; then
  echo "[FAIL] $zero_match smoke category(ies) matched zero tests (silent no-op)"
  exit 1
fi

if [[ $failed -gt 0 ]]; then
  echo "[FAIL] $failed smoke category(ies) failed"
  exit 1
fi

echo "[OK] Invariant smoke PASSED"
