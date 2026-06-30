#!/usr/bin/env bash
# Verify that every Issue number referenced by "// Workaround: ... (Issue #NNNN)" in source
# also appears in docs/vm/WORKAROUNDS.md.
#
# See: Issue #3263
set -euo pipefail

SRC_DIR="subset_julia_vm/src"
WORKAROUNDS_DOC="docs/vm/WORKAROUNDS.md"
ERRORS=0

if [[ ! -f "$WORKAROUNDS_DOC" ]]; then
    echo "ERROR: $WORKAROUNDS_DOC not found."
    exit 1
fi

# Extract unique Issue numbers from // Workaround: ... (Issue #NNNN) comments in source.
# Exclude:
#   - /// doc comments (lines containing ///)
#   - backtick-quoted patterns (documentation examples)
# Use a `while read` loop instead of `mapfile` so this script works on macOS's
# default /bin/bash (3.2), which predates the bash 4 `mapfile` builtin (Issue #3759).
issue_nums=()
while IFS= read -r num; do
    issue_nums+=("$num")
done < <(
    grep -rn "// Workaround:" "$SRC_DIR" --include="*.rs" \
    | grep -v "///" \
    | grep -oE "\(Issue #[0-9]+\)" \
    | grep -oE "[0-9]+" \
    | sort -u
)

for num in "${issue_nums[@]+"${issue_nums[@]}"}"; do
    if ! grep -q "#${num}" "$WORKAROUNDS_DOC"; then
        echo "ERROR: Issue #${num} is referenced by a '// Workaround:' comment in source"
        echo "       but does not appear in $WORKAROUNDS_DOC."
        echo "       Add an entry for Issue #${num} to the WORKAROUNDS.md Summary Table."
        ERRORS=$((ERRORS + 1))
    fi
done

if [[ "$ERRORS" -gt 0 ]]; then
    echo ""
    echo "FAILED: $ERRORS workaround issue(s) missing from $WORKAROUNDS_DOC (Issue #3263)."
    exit 1
fi

echo "OK: all workaround Issue references appear in $WORKAROUNDS_DOC (Issue #3263)."
