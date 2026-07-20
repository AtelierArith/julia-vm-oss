#!/usr/bin/env bash
# test_fixture_julia_parity.sh
#
# Smoke test for scripts/fixture_julia_parity.sh (Issue #4718). Guards
# against silent regressions in the parity helper's summary extraction
# and exit-code contract:
#
#   1. The helper passes on a known-good parity fixture
#      (subset_julia_vm/tests/fixtures/io/summary.jl).
#   2. The helper's extracted "N passed, M failed" summary matches a
#      two-digit pass count (regression guard for the greedy-regex
#      bug fixed during PR #4713 that turned "15 passed, 0 failed"
#      into "5 0").
#   3. Boolean-return fixtures without Test.jl summaries are checked
#      through wrapper final-value extraction against manifest expected.
#
# NAMING: prefix `test_` (not `check_`) keeps it out of the
# `check_*.sh` audit registration (Issue #4714). CI runs this as the
# fixture-parity smoke gate (Issue #8459), while the exhaustive parity helper
# remains directly runnable for focused local investigations.
#
# Usage: bash scripts/test_fixture_julia_parity.sh
#
# Requirements:
#   - julia on PATH
#   - $CARGO_TARGET_DIR/release/sjulia already built (same as the parity helper,
#     or SJULIA_BIN set)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"
SJULIA_BIN="${SJULIA_BIN:-$cargo_target_dir/release/sjulia}"
export SJULIA_BIN

PARITY="scripts/fixture_julia_parity.sh"
KNOWN_GOOD="subset_julia_vm/tests/fixtures/io/summary.jl"
BOOLEAN_RETURN="subset_julia_vm/tests/fixtures/linalg/factorization_inplace_values_7464.jl"

if [[ ! -x "$PARITY" ]]; then
    echo "ERROR: $PARITY not found or not executable." >&2
    exit 2
fi
if [[ ! -f "$KNOWN_GOOD" ]]; then
    echo "ERROR: known-good fixture $KNOWN_GOOD missing — update this test." >&2
    exit 2
fi
if [[ ! -f "$BOOLEAN_RETURN" ]]; then
    echo "ERROR: boolean-return fixture $BOOLEAN_RETURN missing — update this test." >&2
    exit 2
fi

if ! command -v julia >/dev/null 2>&1; then
    echo "SKIP: julia not on PATH — cannot self-test the parity helper." >&2
    exit 0
fi
# Honor the same SJULIA_BIN override as the parity helper itself (Issues
# #8632/#11598), including its CARGO_TARGET_DIR-derived default.
if [[ ! -x "$SJULIA_BIN" ]]; then
    echo "SKIP: sjulia binary not built ($SJULIA_BIN) — run:" >&2
    echo "  cargo build --release -p subset_julia_vm --bin sjulia --features repl" >&2
    echo "  (or set SJULIA_BIN to an existing sjulia binary)" >&2
    exit 0
fi

# Case 1: known-good fixture, expect exit 0.
if ! bash "$PARITY" "$KNOWN_GOOD" >/dev/null 2>&1; then
    echo "FAIL: parity helper failed on known-good fixture $KNOWN_GOOD" >&2
    bash "$PARITY" "$KNOWN_GOOD" >&2 || true
    exit 1
fi
echo "OK: parity helper passes on $KNOWN_GOOD"

# Case 2: two-digit pass count regression guard.
# The greedy-regex bug fixed in PR #4713 turned "15 passed, 0 failed"
# into "5 0" by capturing the trailing "5" of "15". Construct a
# fixture with exactly 15 passing tests and verify the helper reports
# the full "15 passed, 0 failed" pair, not "5 0".
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT
cat > "$tmpdir/two_digit_count.jl" <<'EOF'
using Test
@testset "two-digit count regression guard (Issue #4718)" begin
    for i in 1:15
        @test i == i
    end
end
true
EOF

# Place the fixture under tests/fixtures/ so relative paths inside
# the helper stay sane (the helper does not require this, but it
# matches the documented usage shape).
fixture_dir="subset_julia_vm/tests/fixtures/_parity_self_test_tmp"
rm -rf "$fixture_dir"
mkdir -p "$fixture_dir"
cp "$tmpdir/two_digit_count.jl" "$fixture_dir/two_digit_count.jl"
trap 'rm -rf "$tmpdir" "$fixture_dir"' EXIT

helper_output=$(bash "$PARITY" "$fixture_dir/two_digit_count.jl" 2>&1)
if echo "$helper_output" | grep -q "15 passed, 0 failed"; then
    echo "OK: two-digit (15) pass count extracted correctly"
else
    echo "FAIL: two-digit pass count regression (greedy-regex bug returned)" >&2
    echo "$helper_output" >&2
    exit 1
fi

# Case 3 (Issue #4720): multi-testset fixture. The parity helper
# compares the full sequence of "PASS FAIL" pairs as a single string,
# so a regression that loses or reorders one of the per-testset
# summaries would slip past Case 1 and Case 2. Construct a two-testset
# fixture and assert both summaries (3+0 and 2+0) are reported.
cat > "$tmpdir/multi_testset.jl" <<'EOF'
using Test
@testset "first set" begin
    @test 1 + 1 == 2
    @test 2 + 2 == 4
    @test 3 + 3 == 6
end
@testset "second set" begin
    @test "a" == "a"
    @test 1.0 < 2.0
end
true
EOF
cp "$tmpdir/multi_testset.jl" "$fixture_dir/multi_testset.jl"

helper_output=$(bash "$PARITY" "$fixture_dir/multi_testset.jl" 2>&1)
if ! echo "$helper_output" | grep -q "3 passed, 0 failed"; then
    echo "FAIL: multi-testset first-set summary (3 passed) missing" >&2
    echo "$helper_output" >&2
    exit 1
fi
if ! echo "$helper_output" | grep -q "2 passed, 0 failed"; then
    echo "FAIL: multi-testset second-set summary (2 passed) missing" >&2
    echo "$helper_output" >&2
    exit 1
fi
echo "OK: multi-testset per-testset summaries extracted correctly"

# Case 4 (Issue #8467): boolean-return fixture with no Test.jl summary.
# Direct process exit status is not enough here: the helper must print and
# compare the wrapped final value and the manifest expected value.
helper_output=$(bash "$PARITY" "$BOOLEAN_RETURN" 2>&1)
if ! echo "$helper_output" | grep -q "final value: true"; then
    echo "FAIL: boolean-return fixture final value was not reported" >&2
    echo "$helper_output" >&2
    exit 1
fi
if ! echo "$helper_output" | grep -q "manifest expected: true"; then
    echo "FAIL: boolean-return fixture manifest expected value was not reported" >&2
    echo "$helper_output" >&2
    exit 1
fi
echo "OK: boolean-return fixture final value and manifest expected value reported"

echo "OK: scripts/fixture_julia_parity.sh self-tests pass (Issues #4718, #4720, #8467)."
