#!/usr/bin/env bash
# check_no_panic_in_tests.sh
#
# Detect panic assertion anti-patterns in test code. Catches two variants:
#
# Variant 1 — `=> panic!` (match arm):
#   match result { pat => {} other => panic!("Expected...", other) }
#
# Variant 2 — `} else { panic!` (if-let else branch):
#   if let Pat = val { ... } else { panic!("Expected ...") }
#
# Both are fragile. Prefer:
#
#   assert!(
#       matches!(result, ExpectedVariant(..)),
#       "Expected ExpectedVariant, got {:?}", result
#   );
#
# This script:
# - Searches for both patterns in ALL .rs source files across both src/ and tests/
#   (dedicated test files, inline #[cfg(test)] modules, and integration tests)
# - Skips doc comments (lines starting with /// or //!)
# - Reports unannotated occurrences (missing `// OK: panic!` on the same line)
# - Tracks separate baselines for src/ and tests/ (gradual reduction)
# - Exits 1 if the count in either directory exceeds its baseline (prevents regressions)
# - Exits 0 otherwise (counts at or below baselines)
#
# To exclude a legitimate use (e.g., std::panic::resume_unwind), add:
#   // OK: panic! — <reason>
# on the SAME LINE as the `panic!` call.
#
# Usage:
#   bash scripts/check_no_panic_in_tests.sh
#
# See CLAUDE.md "Test Assertion Style" for preferred patterns.
# Covers: Issue #3053 (tests.rs files), Issue #3090 (inline #[cfg(test)] blocks),
#         Issue #3098 (extended to all .rs files in src/),
#         Issue #3100 (extended to also cover tests/ directory),
#         Issue #3053 variant 2: } else { panic! pattern

set -euo pipefail

SRC_DIR="subset_julia_vm/src"
TESTS_DIR="subset_julia_vm/tests"

# Baselines for unannotated `=> panic!` occurrences.
# Reduce these as tests are refactored. Set to 0 for zero tolerance.
SRC_BASELINE=81      # src/: existing inline/unit-test violations, reduce over time
TESTS_BASELINE=339   # tests/: existing integration test violations, reduce over time

# scan_directory DIR
# Scans all .rs files in DIR for `=> panic!` violations.
# Sets _SCAN_COUNT to the number of violations found.
# Stores violation locations in _SCAN_VIOLATIONS array.
_SCAN_COUNT=0
_SCAN_VIOLATIONS=()

scan_directory() {
    local dir="$1"
    _SCAN_COUNT=0
    _SCAN_VIOLATIONS=()

    if [[ ! -d "$dir" ]]; then
        return
    fi

    while IFS= read -r srcfile; do
        # Variant 1: `=> panic!` (match arm anti-pattern)
        while IFS= read -r line; do
            lineno=$(echo "$line" | cut -d: -f1)
            content=$(echo "$line" | cut -d: -f2-)

            # Skip doc comments (/// or //!) — these are examples, not real code
            if echo "$content" | grep -qE '^\s*//[/!]'; then
                continue
            fi

            # Skip if the line itself contains an OK annotation
            if echo "$content" | grep -qE '//\s*OK:\s*panic!'; then
                continue
            fi

            # Skip resume_unwind (legitimate panic propagation)
            if echo "$content" | grep -q 'resume_unwind'; then
                continue
            fi

            _SCAN_VIOLATIONS+=("$srcfile:$lineno (=> panic!)")
        done < <(grep -n '=> panic!' "$srcfile" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' || true)

        # Variant 2: `} else { ... panic!` on the next line (if-let else branch anti-pattern)
        # Use awk to detect `panic!` on the line immediately after `} else {` or `else {`
        while IFS= read -r violation_line; do
            _SCAN_VIOLATIONS+=("$srcfile:$violation_line (else { panic! })")
        done < <(awk '
            /[}]?\s*else\s*\{[[:space:]]*$/ { else_line = NR; next }
            /panic!/ {
                if (NR == else_line + 1) {
                    if ($0 !~ /\/\/ OK: panic!/ && $0 !~ /resume_unwind/ && $0 !~ /^\s*\/\/[\/!]/) {
                        print NR
                    }
                }
            }
            { else_line = 0 }
        ' "$srcfile" 2>/dev/null || true)
    done < <(find "$dir" -name "*.rs" -type f 2>/dev/null)

    _SCAN_COUNT="${#_SCAN_VIOLATIONS[@]}"
}

# --- Scan src/ ---
scan_directory "$SRC_DIR"
SRC_COUNT=$_SCAN_COUNT
SRC_VIOLATIONS=("${_SCAN_VIOLATIONS[@]+"${_SCAN_VIOLATIONS[@]}"}")

if [ "$SRC_COUNT" -gt 0 ]; then
    echo "=== $SRC_DIR: $SRC_COUNT violations (baseline: $SRC_BASELINE) ==="
    for loc in "${SRC_VIOLATIONS[@]}"; do
        echo "  $loc"
    done
fi

# --- Scan tests/ ---
scan_directory "$TESTS_DIR"
TESTS_COUNT=$_SCAN_COUNT
TESTS_VIOLATIONS=("${_SCAN_VIOLATIONS[@]+"${_SCAN_VIOLATIONS[@]}"}")

if [ "$TESTS_COUNT" -gt "$TESTS_BASELINE" ]; then
    echo "=== $TESTS_DIR: $TESTS_COUNT violations (baseline: $TESTS_BASELINE) ==="
    for loc in "${TESTS_VIOLATIONS[@]}"; do
        echo "  $loc"
    done
fi

# --- Summary and exit ---
failed=0

if [ "$SRC_COUNT" -gt "$SRC_BASELINE" ]; then
    echo ""
    echo "ERROR: $SRC_DIR count ($SRC_COUNT) exceeds baseline ($SRC_BASELINE). New => panic! added!"
    failed=1
fi

if [ "$TESTS_COUNT" -gt "$TESTS_BASELINE" ]; then
    echo ""
    echo "ERROR: $TESTS_DIR count ($TESTS_COUNT) exceeds baseline ($TESTS_BASELINE). New => panic! added!"
    failed=1
fi

if [ "$failed" -eq 1 ]; then
    echo ""
    echo "Preferred alternative:"
    echo "  assert!(matches!(result, ExpectedVariant(..)), \"Expected ..., got {:?}\", result);"
    echo ""
    echo "To exempt a legitimate panic!, add '// OK: panic! — <reason>' on the same line."
    echo "See CLAUDE.md 'Test Assertion Style' for details."
    exit 1
fi

echo "OK: => panic! counts within baselines (src/: $SRC_COUNT/$SRC_BASELINE, tests/: $TESTS_COUNT/$TESTS_BASELINE)"
if [ "$SRC_COUNT" -eq 0 ] && [ "$TESTS_COUNT" -eq 0 ]; then
    echo "    Zero tolerance enforced across all directories."
else
    echo "    Note: existing violations remain — reduce baselines over time."
fi

exit 0
