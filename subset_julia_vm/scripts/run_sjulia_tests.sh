#!/bin/bash
#
# Run fixture tests with sjulia (SubsetJuliaVM CLI)
#
# Usage:
#   ./scripts/run_sjulia_tests.sh              # Run all tests
#   ./scripts/run_sjulia_tests.sh --json       # Output JSON for comparison
#   ./scripts/run_sjulia_tests.sh test_name    # Run specific test

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
FIXTURES_DIR="$PROJECT_DIR/tests/fixtures"
MANIFEST_PATH="$FIXTURES_DIR/manifest.toml"

# Build sjulia if needed
cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml" --bin sjulia --features "parser,repl" 2>/dev/null || {
    echo "Building sjulia..."
    cargo build --manifest-path "$PROJECT_DIR/Cargo.toml" --bin sjulia --features "parser,repl"
}

SJULIA="$PROJECT_DIR/target/release/sjulia"
if [ ! -f "$SJULIA" ]; then
    SJULIA="$PROJECT_DIR/target/debug/sjulia"
fi

if [ ! -f "$SJULIA" ]; then
    echo "Error: sjulia binary not found. Run 'cargo build --bin sjulia --features parser,repl'"
    exit 1
fi

JSON_OUTPUT=false
FILTER=""

for arg in "$@"; do
    case $arg in
        --json)
            JSON_OUTPUT=true
            ;;
        -*)
            ;;
        *)
            FILTER="$arg"
            ;;
    esac
done

# Parse manifest and run tests
PASSED=0
FAILED=0
RESULTS=""

# Read tests from manifest using simple parsing
# Format: [[tests]]\nname = "..."\nfile = "..."\nexpected = ...

current_name=""
current_file=""
current_expected=""

run_single_test() {
    local name="$1"
    local file="$2"
    local expected="$3"

    if [ -n "$FILTER" ] && [[ ! "$name" == *"$FILTER"* ]]; then
        return
    fi

    local filepath="$FIXTURES_DIR/$file"
    if [ ! -f "$filepath" ]; then
        echo "  ✗ $name (file not found: $file)"
        ((FAILED++))
        return
    fi

    # Run with sjulia and capture output
    local result
    result=$("$SJULIA" "$filepath" 2>&1) || true

    # Extract the last numeric value from output
    local actual
    actual=$(echo "$result" | grep -E '^-?[0-9]+\.?[0-9]*$' | tail -1)

    if [ -z "$actual" ]; then
        actual="NaN"
    fi

    # Compare with expected (using bc for floating point)
    local diff
    if [ "$actual" = "NaN" ]; then
        diff="inf"
    else
        diff=$(echo "scale=15; x=$actual - $expected; if (x < 0) -x else x" | bc -l 2>/dev/null || echo "inf")
    fi

    local epsilon="0.0000000001"
    local passed
    passed=$(echo "$diff < $epsilon" | bc -l 2>/dev/null || echo "0")

    if [ "$passed" = "1" ]; then
        echo "  ✓ $name"
        ((PASSED++))
        RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":true,\"expected\":$expected,\"actual\":$actual},"
    else
        echo "  ✗ $name"
        echo "      Expected: $expected"
        echo "      Actual:   $actual"
        ((FAILED++))
        RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"expected\":$expected,\"actual\":$actual},"
    fi
}

echo "============================================================"
echo "sjulia Fixture Test Results"
echo "============================================================"

# Parse TOML and run tests
while IFS= read -r line; do
    if [[ "$line" == '[[tests]]' ]]; then
        # Run previous test if exists
        if [ -n "$current_name" ]; then
            run_single_test "$current_name" "$current_file" "$current_expected"
        fi
        current_name=""
        current_file=""
        current_expected=""
    elif [[ "$line" =~ ^name\ *=\ *\"(.*)\"$ ]]; then
        current_name="${BASH_REMATCH[1]}"
    elif [[ "$line" =~ ^file\ *=\ *\"(.*)\"$ ]]; then
        current_file="${BASH_REMATCH[1]}"
    elif [[ "$line" =~ ^expected\ *=\ *([0-9.-]+)$ ]]; then
        current_expected="${BASH_REMATCH[1]}"
    fi
done < "$MANIFEST_PATH"

# Run last test
if [ -n "$current_name" ]; then
    run_single_test "$current_name" "$current_file" "$current_expected"
fi

TOTAL=$((PASSED + FAILED))

echo "------------------------------------------------------------"
echo "Total: $TOTAL | Passed: $PASSED | Failed: $FAILED"
echo "============================================================"

if [ "$JSON_OUTPUT" = true ]; then
    # Remove trailing comma and wrap in JSON
    RESULTS="${RESULTS%,}"
    echo "{\"results\":[$RESULTS]}"
fi

if [ "$FAILED" -gt 0 ]; then
    exit 1
fi
