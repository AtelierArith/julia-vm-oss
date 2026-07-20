#!/usr/bin/env bash
# check_vmerror_classification.sh
#
# Audit VmError variant classification in vm/exec/.
#
# Every `return Err(VmError::TypeError(...))`  in vm/exec/ must be annotated with
# a comment on the line immediately before it explaining the intent:
#
#   // User-visible: <reason why user code can trigger this>
#   return Err(VmError::TypeError(...))
#
# If the error is a COMPILER INVARIANT (user code cannot trigger it), change to:
#
#   // INTERNAL: <compiler invariant that was violated>
#   return Err(VmError::InternalError(...))
#
# This script reports unannotated occurrences and tracks progress toward full annotation.
# It exits with 1 only if the unannotated count INCREASES above the current baseline.
# To enforce zero tolerance, set BASELINE=0.
#
# Usage:
#   bash scripts/check_vmerror_classification.sh
#
# See docs/vm/PANIC_FREE.md for the VmError classification guide.

set -euo pipefail

EXEC_DIR="subset_julia_vm_vm/src/vm/exec"

# Current baseline of unannotated occurrences.
# Reduce this as annotations are added. Set to 0 for strict enforcement.
BASELINE=49

UNANNOTATED=()

while IFS= read -r line; do
    file=$(echo "$line" | cut -d: -f1)
    lineno=$(echo "$line" | cut -d: -f2)

    # Read the line before the return Err
    prev_line=$(sed -n "$((lineno - 1))p" "$file" 2>/dev/null || true)

    # Check if previous line has // User-visible: or // INTERNAL:
    if echo "$prev_line" | grep -qE '//\s*(User-visible:|INTERNAL:)'; then
        : # Annotated — OK
    else
        UNANNOTATED+=("$file:$lineno")
    fi
done < <(grep -rn 'return Err(VmError::TypeError' "$EXEC_DIR" --include='*.rs' | \
         grep -v '#\[cfg(test)\]' | \
         sed 's/\x1b\[[0-9;]*m//g')  # strip ANSI color codes

COUNT="${#UNANNOTATED[@]}"

if [ "$COUNT" -gt 0 ]; then
    echo "Unannotated return Err(VmError::TypeError) in $EXEC_DIR: $COUNT / $BASELINE baseline"
    echo ""
    for loc in "${UNANNOTATED[@]}"; do
        echo "  $loc"
    done
    echo ""
    echo "Add one of these comments on the line before each return:"
    echo "  // User-visible: <reason> (keep as TypeError)"
    echo "  // INTERNAL: <compiler invariant> (change to InternalError)"
    echo ""
    echo "See docs/vm/PANIC_FREE.md for the VmError classification guide."
fi

if [ "$COUNT" -gt "$BASELINE" ]; then
    echo ""
    echo "ERROR: Count ($COUNT) exceeds baseline ($BASELINE). New unannotated TypeError added!"
    echo "Please annotate all new return Err(VmError::TypeError) before merging."
    exit 1
fi

if [ "$COUNT" -eq 0 ]; then
    echo "OK: All return Err(VmError::TypeError) in $EXEC_DIR are annotated."
else
    echo "OK: unannotated TypeError count is within baseline; reduce BASELINE as annotations land."
fi

exit 0
