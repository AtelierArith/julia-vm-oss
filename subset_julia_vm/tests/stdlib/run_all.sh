#!/bin/bash
# Run all stdlib tests for SubsetJuliaVM
# Usage: ./run_all.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SJULIA="${SCRIPT_DIR}/../../target/release/sjulia"

# Check if sjulia exists
if [ ! -f "$SJULIA" ]; then
    echo "Error: sjulia not found at $SJULIA"
    echo "Please build with: cargo build --release --features parser,repl"
    exit 1
fi

echo "Running stdlib tests..."
echo "==============================="

TESTS=(
    "test_Statistics.jl"
    "test_Random.jl"
    "test_InteractiveUtils.jl"
)

PASSED=0
FAILED=0

for test in "${TESTS[@]}"; do
    echo ""
    echo "Running $test..."
    if "$SJULIA" "$SCRIPT_DIR/$test" 2>&1; then
        ((PASSED++))
    else
        echo "FAILED: $test"
        ((FAILED++))
    fi
done

echo ""
echo "==============================="
echo "Results: $PASSED passed, $FAILED failed"

if [ $FAILED -gt 0 ]; then
    exit 1
fi
