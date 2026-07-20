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

SRCDIR="subset_julia_vm_vm/src/vm"

if [[ ! -d "$SRCDIR" ]]; then
    echo "ERROR: $SRCDIR not found. Run from the repository root." >&2
    exit 1
fi

# bash 3.2 compatible: no associative arrays (`declare -A` is a bash-4 builtin
# flag that exits 2 on macOS stock /bin/bash — Issue #9461). We instead write a
# `BuiltinId::Variant<TAB>file` pair per line (deduplicated per file via
# `sort -u`) to a temp file, then group by variant with plain cut/sort/uniq/awk.
pairs_file="$(mktemp)"
trap 'rm -f "$pairs_file"' EXIT

for f in "$SRCDIR"/builtins_*.rs; do
    # Skip the fallback handler file — it intentionally shadows specialized handlers
    [[ "$f" == *"builtins_exec.rs" ]] && continue

    # Extract BuiltinId::Variant references from non-comment lines only,
    # deduplicated per file, and record one `variant<TAB>file` pair per line.
    # (Exclude lines that are entirely comments: optional whitespace + //.)
    # NOTE: feed the loop via process substitution `< <(...)`, NOT a pipe — a
    # trailing `grep | while` would let a no-match grep (exit 1) trip
    # `set -o pipefail` + `set -e` for files with no BuiltinId reference.
    while IFS= read -r id; do
        [[ -n "$id" ]] && printf '%s\t%s\n' "$id" "$f" >> "$pairs_file"
    done < <(
        grep -v '^[[:space:]]*//' "$f" 2>/dev/null \
            | grep -oE 'BuiltinId::[A-Za-z_]+' \
            | sort -u
    )
done

# A variant handled in more than one specialized file appears on >1 pair line
# (already unique per file). `cut`+`sort`+`uniq -d` lists exactly those.
dup_ids="$(cut -f1 "$pairs_file" | sort | uniq -d)"

found=0
while IFS= read -r id; do
    [[ -n "$id" ]] || continue
    echo "DUPLICATE: $id"
    awk -F'\t' -v want="$id" '$1 == want { print "  " $2 }' "$pairs_file"
    found=1
done <<< "$dup_ids"

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
