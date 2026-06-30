#!/bin/bash
#
# Compare fixture test results across all environments:
# - cargo test (Rust/SubsetJuliaVM)
# - julia (Official Julia)
# - sjulia (SubsetJuliaVM CLI)
#
# Usage:
#   ./scripts/compare_all.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "============================================================"
echo "SubsetJuliaVM Compatibility Test Suite"
echo "============================================================"
echo ""

# Track overall results
ALL_PASSED=true

# 1. Run cargo test (fixture tests only)
echo ">>> Running cargo test (fixture_tests)..."
echo "------------------------------------------------------------"
if cargo test --manifest-path "$PROJECT_DIR/Cargo.toml" --test fixture_tests 2>&1; then
    echo "✓ cargo test: PASSED"
else
    echo "✗ cargo test: FAILED"
    ALL_PASSED=false
fi
echo ""

# 2. Run Julia tests
echo ">>> Running Julia tests..."
echo "------------------------------------------------------------"
if command -v julia &> /dev/null; then
    if julia "$SCRIPT_DIR/run_julia_tests.jl" 2>&1; then
        echo "✓ julia: PASSED"
    else
        echo "✗ julia: FAILED"
        ALL_PASSED=false
    fi
else
    echo "⚠ julia: NOT INSTALLED (skipped)"
fi
echo ""

# 3. Run sjulia tests
echo ">>> Running sjulia tests..."
echo "------------------------------------------------------------"
if bash "$SCRIPT_DIR/run_sjulia_tests.sh" 2>&1; then
    echo "✓ sjulia: PASSED"
else
    echo "✗ sjulia: FAILED"
    ALL_PASSED=false
fi
echo ""

# Summary
echo "============================================================"
echo "SUMMARY"
echo "============================================================"
if [ "$ALL_PASSED" = true ]; then
    echo "✓ All tests passed across all environments!"
    exit 0
else
    echo "✗ Some tests failed. Check output above for details."
    exit 1
fi
