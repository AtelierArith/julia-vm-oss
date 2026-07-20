#!/usr/bin/env bash
# check_workarounds_documented.sh
#
# Verify that every "// Workaround:" comment in workspace Rust sources carries
# an Issue link of the form "(Issue #NNNN)".
#
# CLAUDE.md convention (Workaround Management section):
#   Every workaround must be annotated as:
#   // Workaround: <description> (Issue #XXXX)
#
# Usage: run from the repository root
#   bash scripts/check_workarounds_documented.sh
#
# Exit code: 0 = OK, 1 = undocumented workaround(s) found

set -euo pipefail

SRC_DIRS=(subset_julia_vm*/src)

if [[ ! -d "subset_julia_vm/src" ]]; then
    echo "ERROR: subset_julia_vm/src not found. Run this script from the repository root."
    exit 1
fi

# Find workaround comments that do NOT contain "(Issue #<digits>)"
# Exclude:
#   - Doc comments (/// ...) that mention // Workaround: as example text
#   - Backtick-quoted patterns (`// Workaround: ...`) in doc strings
hits=$(grep -rn "// Workaround:" "${SRC_DIRS[@]}" --include="*.rs" \
    | grep -v "Issue #[0-9]" \
    | grep -v "^[^:]*:.*///" \
    | grep -v '`// Workaround:' \
    || true)

if [[ -n "$hits" ]]; then
    echo "ERROR: Workaround comment(s) found without an Issue link."
    echo "Each // Workaround: comment must end with (Issue #NNNN)."
    echo "See CLAUDE.md 'Workaround Management' and docs/vm/WORKAROUNDS.md."
    echo ""
    echo "Offending lines:"
    echo "$hits"
    exit 1
fi

echo "OK: all // Workaround: comments carry an Issue link (Issue #3179)."
