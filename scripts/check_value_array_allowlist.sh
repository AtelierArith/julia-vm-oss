#!/usr/bin/env bash
# Array carrier audit (Issues #4568, #6805, #6723, #6807).
#
# Two policies are enforced across `subset_julia_vm/src` and
# `subset_julia_vm/tests`:
#
#   1. `Value::Array`     — zero-match. The legacy array carrier variant was
#      retired (#4568). Any reuse is a regression.
#
#   2. `Value::ExprArgs`  — confinement allowlist (#6807). The Rc-backed mutable
#      array carrier (renamed from `NativeArray`) is now ACCEPTED as the dedicated
#      `expr.args` representation: its sole runtime origin is `expr.args` (mutable
#      `Vector{Any}` AST args of an `Expr`), which need auto-freed reference
#      semantics because `struct_heap` has no per-value GC — a heap-`StructRef`
#      wrapper would leak one slot per transient `Expr` node. All *general* array
#      values are the MemoryRef-backed pure-Julia `Array{T,N}` wrapper; a plain
#      array program produces zero `Value::ExprArgs` carriers.
#
#      This audit pins the variant text to an explicit allowlist (the variant
#      definition + the `native_array_*` converter helpers + the carrier unit
#      tests) so the carrier stays confined to its single legitimate role:
#        - a file NOT on the allowlist that uses the variant fails the audit
#          (a new carrier site outside `expr.args` must be justified and added
#          here), and
#        - an allowlist entry that no longer uses the variant fails too (stale
#          entry — delete it).
#      Consumers handle the carrier through the `native_array_*` helpers (which
#      live in the allowlisted converter hub), not by matching the variant text,
#      so the allowlist is the carrier's confinement boundary, not a count of
#      every site it flows through.

set -euo pipefail

errors=0

# --- Policy 1: Value::Array zero-match (#4568) -------------------------------
array_matches="$(rg -n '\bValue::Array' subset_julia_vm/src subset_julia_vm/tests --glob '*.rs' || true)"
if [[ -n "$array_matches" ]]; then
    echo "ERROR: unexpected Value::Array use (retired variant, Issue #4568):"
    echo "$array_matches"
    echo ""
    echo "Use Memory primitives, Pure Julia Array wrappers, or explicit"
    echo "ExprArgs carrier converters instead."
    errors=$((errors + 1))
fi

# --- Policy 2: Value::ExprArgs confinement allowlist (#6807) -----------------
# The carrier is confined to these files: the variant definition + enum arm,
# the `native_array_*` converter hub, and the carrier unit tests. A new file
# matching the variant text is a new carrier site outside `expr.args` and must
# be justified; a no-longer-matching entry is stale and should be removed.
EXPR_ARGS_ALLOWLIST=(
    subset_julia_vm/src/lowering/macro_runtime.rs
    subset_julia_vm/src/vm/value/value_enum.rs
    subset_julia_vm/src/vm/value/array_value/mod.rs
    subset_julia_vm/src/vm/frame.rs
)

carrier_files="$(rg -l 'Value::ExprArgs' subset_julia_vm/src subset_julia_vm/tests --glob '*.rs' | sort -u || true)"

# 2a. New (non-allowlisted) usages.
while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    found=0
    for allowed in "${EXPR_ARGS_ALLOWLIST[@]}"; do
        [[ "$file" == "$allowed" ]] && { found=1; break; }
    done
    if [[ "$found" -eq 0 ]]; then
        echo "ERROR: new Value::ExprArgs use outside the carrier confinement allowlist: $file"
        rg -n 'Value::ExprArgs' "$file" || true
        echo "       The ExprArgs carrier is confined to the expr.args representation."
        echo "       Route general arrays through the MemoryRef-backed Array{T,N} wrapper,"
        echo "       or (if genuinely carrier-related) add the file to EXPR_ARGS_ALLOWLIST"
        echo "       with justification (#6807)."
        errors=$((errors + 1))
    fi
done <<< "$carrier_files"

# 2b. Stale allowlist entries.
for allowed in "${EXPR_ARGS_ALLOWLIST[@]}"; do
    if ! grep -qxF "$allowed" <<< "$carrier_files"; then
        echo "ERROR: stale Value::ExprArgs allowlist entry (no match left): $allowed"
        echo "       Remove it from EXPR_ARGS_ALLOWLIST in $0 (#6807)."
        errors=$((errors + 1))
    fi
done

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: array carrier audit failed (Issues #4568 / #6807)."
    exit 1
fi

echo "OK: array carrier audit passed."
echo "    - Value::Array zero-match (#4568)"
echo "    - Value::ExprArgs confinement allowlist, ${#EXPR_ARGS_ALLOWLIST[@]} files (#6807)"
