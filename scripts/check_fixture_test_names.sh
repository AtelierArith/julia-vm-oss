#!/usr/bin/env bash
# check_fixture_test_names.sh
#
# Detect duplicate fixture test names across all category manifest.toml files.
#
# When two categories define tests with the same `name` field, the fixture test
# runner silently loads whichever category happens to be scanned first by the
# filesystem, producing wrong results with no error or warning (Issue #3135).
#
# This script scans all manifest.toml files under
#   subset_julia_vm/tests/fixtures/*/manifest.toml
# and reports any `name = "..."` values that appear more than once.
#
# Usage: run from the repository root
#   bash scripts/check_fixture_test_names.sh
#
# Exit code: 0 = no duplicates, 1 = duplicates found

set -euo pipefail

FIXTURES_DIR="subset_julia_vm/tests/fixtures"

if [[ ! -d "$FIXTURES_DIR" ]]; then
    echo "ERROR: fixtures directory not found: $FIXTURES_DIR"
    echo "Run this script from the repository root."
    exit 1
fi

# Collect all name = "..." lines from category manifests
# (excluding the root manifest.toml which has a different structure)
duplicates=$(
    grep -rh '^name = ' "$FIXTURES_DIR"/*/manifest.toml 2>/dev/null \
        | sort \
        | uniq -d
)

if [[ -n "$duplicates" ]]; then
    echo "ERROR: duplicate fixture test names found across categories:"
    while IFS= read -r dup; do
        echo "  $dup"
        # Show which files contain this name
        grep -rl "$dup" "$FIXTURES_DIR"/*/manifest.toml 2>/dev/null \
            | sed 's/^/    -> /'
    done <<< "$duplicates"
    echo ""
    echo "Fix: rename the duplicate tests so each name is unique across all categories."
    echo "Tip: prefix the test name with the category name (e.g. 'meta_isidentifier_validation')."
    exit 1
fi

echo "OK: all fixture test names are unique across categories (Issue #3135)."
