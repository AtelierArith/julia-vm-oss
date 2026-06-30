#!/usr/bin/env bash
# check_builtin_duplicates.sh
#
# Detect BuiltinId variants handled in more than one SPECIALIZED builtins_*.rs file.
#
# A BuiltinId appearing in multiple specialized handler files causes the
# dispatch_builtin! macro to silently ignore all but the first match, creating
# dead code and potential behavioral divergence (see Issues #3026, #3031).
#
# NOTE: builtins_exec.rs is excluded from this check — it contains legacy
# fallback handlers that are intentionally shadowed by specialized handlers.
# The dispatch chain calls specialized handlers BEFORE the fallback match in
# builtins_exec.rs, so duplicates with builtins_exec.rs are not harmful.
#
# Only duplicates AMONG the specialized files (builtins_math, builtins_io,
# builtins_collections, builtins_arrays, etc.) are flagged as errors.
#
# Usage: run from the repository root
#   ./scripts/check_builtin_duplicates.sh
#
# Exit code: 0 if no duplicates found, 1 if duplicates detected.

set -euo pipefail

SRCDIR="subset_julia_vm/src/vm"

if [[ ! -d "$SRCDIR" ]]; then
    echo "ERROR: $SRCDIR not found. Run from the repository root." >&2
    exit 1
fi

declare -A id_files

for f in "$SRCDIR"/builtins_*.rs; do
    # Skip the fallback handler file — it intentionally shadows specialized handlers
    [[ "$f" == *"builtins_exec.rs" ]] && continue

    # Extract BuiltinId::Variant references from non-comment lines only,
    # deduplicated per file.
    while IFS= read -r id; do
        [[ -n "$id" ]] && id_files["$id"]+="$f "
    done < <(
        # Exclude lines that are entirely comments (optional whitespace + //)
        grep -v '^\s*//' "$f" 2>/dev/null \
        | grep -oE 'BuiltinId::[A-Za-z_]+' \
        | sort -u
    )
done

found=0
for id in "${!id_files[@]}"; do
    files="${id_files[$id]}"
    count=$(echo "$files" | wc -w | tr -d ' ')
    if [[ "$count" -gt 1 ]]; then
        echo "DUPLICATE: $id"
        for f in $files; do
            echo "  $f"
        done
        found=1
    fi
done

if [[ "$found" -eq 0 ]]; then
    echo "OK: no duplicate BuiltinId handlers found in specialized builtins_*.rs files"
    exit 0
else
    echo ""
    echo "ERROR: duplicate BuiltinId handlers detected among specialized handler files."
    echo "Each BuiltinId must be owned by exactly one specialized handler file."
    echo "The dispatch_builtin! chain uses first-match-wins semantics, so the"
    echo "handler in the LATER file is silently unreachable dead code."
    echo "See docs/vm/BUILTIN_OWNERSHIP.md for the authoritative ownership table."
    exit 1
fi
