#!/bin/bash
# Run all base function tests using sjulia
# Usage: ./run_all.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SJULIA="${SCRIPT_DIR}/../../target/release/sjulia"

# Check if sjulia exists
if [ ! -f "$SJULIA" ]; then
    echo "Error: sjulia not found at $SJULIA"
    echo "Please build first: cargo build --release"
    exit 1
fi

echo "Running base function tests..."
echo "==============================="

PASSED=0
FAILED=0
TESTS=(
    "test_operators.jl"
    "test_number.jl"
    "test_bool.jl"
    "test_math.jl"
    "test_intfuncs.jl"
    "test_array.jl"
    "test_statistics.jl"
    "test_sort.jl"
    "test_set.jl"
)

for test in "${TESTS[@]}"; do
    echo ""
    echo "Running $test..."
    if "$SJULIA" "$SCRIPT_DIR/$test"; then
        PASSED=$((PASSED + 1))
    else
        FAILED=$((FAILED + 1))
        echo "FAILED: $test"
    fi
done

echo ""
echo "==============================="
echo "Results: $PASSED passed, $FAILED failed"

if [ $FAILED -gt 0 ]; then
    exit 1
fi
