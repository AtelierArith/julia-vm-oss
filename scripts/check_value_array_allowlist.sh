#!/usr/bin/env bash
# Array carrier audit (Issues #4568, #6805, #6723, #6807, #8918).
#
# One policy is enforced across every workspace crate's `src/` tree and
# `subset_julia_vm/tests`:
#
#   `Value::Array` — zero-match. The legacy array carrier variant was retired
#   (#4568). Any reuse is a regression. This is a deleted-variant guard: the
#   variant no longer exists, so any textual reappearance is a stray reference
#   that must be removed.
#
# RETIRED — `Value::ExprArgs` confinement allowlist (#6807 → #8918):
#   The former Policy 2 grep-ratcheted the Rc-backed `expr.args` carrier
#   (`Value::ExprArgs`) against a hand-maintained `EXPR_ARGS_ALLOWLIST` of files.
#   That confinement is now a **type**, not a grep: the carrier payload is the
#   private-field witness newtype `ExprArgsCarrier` (defined in
#   `subset_julia_vm_bytecode/src/value/array_value/mod.rs`). `Value::ExprArgs`
#   can only be constructed / destructured through the `native_array_*` hub
#   helpers in that module — an off-hub carrier site is a **compile error**, not
#   an audit failure. The compiler checks this on every build, so no allowlist,
#   no variant-text grep, and no per-file justification comments are needed. This
#   follows the `Resolved` newtype template (#8642) that retired the StructRef
#   display grep. See docs/vm/CODE_AUDITS.md.

set -euo pipefail

errors=0

# --- Policy: Value::Array zero-match (#4568) --------------------------------
array_matches="$(rg -n '\bValue::Array' \
    subset_julia_vm/src \
    subset_julia_vm_lowering/src \
    subset_julia_vm_compile/src \
    subset_julia_vm_vm/src \
    subset_julia_vm_bytecode/src \
    subset_julia_vm_types/src \
    subset_julia_vm_ir/src \
    subset_julia_vm_ffi/src \
    subset_julia_vm_parser/src \
    subset_julia_vm_web/src \
    subset_julia_vm/tests \
    --glob '*.rs' || true)"
if [[ -n "$array_matches" ]]; then
    echo "ERROR: unexpected Value::Array use (retired variant, Issue #4568):"
    echo "$array_matches"
    echo ""
    echo "Use Memory primitives, Pure Julia Array wrappers, or the ExprArgs"
    echo "carrier converters (native_array_* helpers) instead."
    errors=$((errors + 1))
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: array carrier audit failed (Issue #4568)."
    exit 1
fi

echo "OK: array carrier audit passed."
echo "    - Value::Array zero-match (#4568)"
echo "    - Value::ExprArgs confinement is now type-enforced (ExprArgsCarrier newtype, #8918)"
