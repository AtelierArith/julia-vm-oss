#!/usr/bin/env bash
# Verify that every file with a "// @generated" header in subset_julia_vm/src/
# also carries a "Re-generate with: <script>" comment AND that the referenced
# script actually exists on disk.
#
# See: Issue #3175
set -euo pipefail

SRC_DIR="subset_julia_vm/src"
ERRORS=0

while IFS= read -r -d '' file; do
    # Only consider files where the FIRST line is a // @generated header
    first_line=$(head -1 "$file")
    if [[ "$first_line" != "// @generated"* ]]; then
        continue
    fi

    # Check that a "Re-generate with:" comment exists somewhere near the top
    regen_line=$(grep -n "Re-generate with:" "$file" | head -1 || true)
    if [[ -z "$regen_line" ]]; then
        echo "ERROR: $file has '// @generated' header but no 'Re-generate with:' comment."
        ERRORS=$((ERRORS + 1))
        continue
    fi

    # Extract the script path from the "Re-generate with: <cmd>" line
    # Expected format: "// Re-generate with: <command> [args...]"
    # We look for a path ending in .py, .sh, .jl, or .rb
    script_path=$(echo "$regen_line" | sed 's/.*Re-generate with:[[:space:]]*//' | awk '{print $2}')
    if [[ -z "$script_path" ]]; then
        # Try awk on the full command (first token after "Re-generate with:")
        script_path=$(echo "$regen_line" | sed 's/.*Re-generate with:[[:space:]]*//' | awk '{print $1}')
    fi

    if [[ -n "$script_path" && ! -f "$script_path" ]]; then
        echo "ERROR: $file references '$script_path' in 'Re-generate with:' but the file does not exist."
        ERRORS=$((ERRORS + 1))
        continue
    fi

    echo "OK: $file"
done < <(find "$SRC_DIR" -name "*.rs" -print0)

if [[ "$ERRORS" -gt 0 ]]; then
    echo ""
    echo "FAILED: $ERRORS @generated file(s) failed validation (Issue #3175)."
    exit 1
fi

echo "OK: all @generated files pass validation (Issue #3175)."
