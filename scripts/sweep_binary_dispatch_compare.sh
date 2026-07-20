#!/usr/bin/env bash
# sweep_binary_dispatch_compare.sh — full-fixture sweep with SJULIA_BINARY_DISPATCH_COMPARE=1
#
# Runs every fixture under sjulia with the binary-dispatch comparison mode
# enabled (Issue #8620, parent #8609).  All stderr output is collected;
# lines prefixed with SJULIA_BINARY_DISPATCH_COMPARE are extracted and
# written to a divergence report.
#
# Usage:
#   bash scripts/sweep_binary_dispatch_compare.sh [--out <report_file>]
#
# Prerequisites:
#   cargo build --release -p subset_julia_vm --bin sjulia --features repl
#   (or the binary exists at target/release/sjulia)
#
# Output:
#   <report_file> (default: /tmp/binary_dispatch_compare_report.txt)
#   Summary statistics are printed to stdout.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SJULIA="$REPO_ROOT/target/release/sjulia"
FIXTURE_ROOT="$REPO_ROOT/subset_julia_vm/tests/fixtures"

# Parse arguments
OUT_FILE="/tmp/binary_dispatch_compare_report.txt"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --out)
            OUT_FILE="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

if [[ ! -x "$SJULIA" ]]; then
    echo "sjulia binary not found at $SJULIA" >&2
    echo "Build with: cargo build --release -p subset_julia_vm --bin sjulia --features repl" >&2
    exit 1
fi

echo "Binary: $SJULIA"
echo "Fixture root: $FIXTURE_ROOT"
echo "Report: $OUT_FILE"
echo ""

# Clear output file
> "$OUT_FILE"

# Counters
total_fixtures=0
fixtures_with_divergence=0

# Run every .jl fixture file
while IFS= read -r -d '' fixture; do
    total_fixtures=$((total_fixtures + 1))

    # Run sjulia with the compare mode enabled, capturing stderr
    stderr_output=$(
        SJULIA_BINARY_DISPATCH_COMPARE=1 \
        timeout 30 "$SJULIA" "$fixture" 2>&1 >/dev/null || true
    )

    # Extract divergence lines
    divergence_lines=$(echo "$stderr_output" | grep "^SJULIA_BINARY_DISPATCH_COMPARE:" || true)

    if [[ -n "$divergence_lines" ]]; then
        fixtures_with_divergence=$((fixtures_with_divergence + 1))
        echo "=== $fixture ===" >> "$OUT_FILE"
        echo "$divergence_lines" >> "$OUT_FILE"
        echo "" >> "$OUT_FILE"
    fi
done < <(find "$FIXTURE_ROOT" -name "*.jl" -print0 | sort -z)

echo "Sweep complete."
echo "  Total fixtures: $total_fixtures"
echo "  Fixtures with divergences: $fixtures_with_divergence"
echo ""

if [[ -s "$OUT_FILE" ]]; then
    echo "Divergence summary (top patterns):"
    grep "^SJULIA_BINARY_DISPATCH_COMPARE:" "$OUT_FILE" | \
        sed 's/ compile=.*$//' | \
        sort | uniq -c | sort -rn | head -20
    echo ""
    echo "Full divergence report written to: $OUT_FILE"
    echo ""
    echo "Total divergence lines: $(grep -c '^SJULIA_BINARY_DISPATCH_COMPARE:' "$OUT_FILE" || echo 0)"
else
    echo "No divergences found — resolver and compile-time agree on all fixtures."
fi
