#!/usr/bin/env bash
# Issue #3953/#4018 - keep array literal builders on the Memory-first helper.
#
# Array literals compile to NewArray/PushElem/FinalizeArray and
# NewArrayTyped/PushElemTyped/FinalizeArrayTyped. The builders may still return
# the transitional Value::Array container, but their storage should be allocated
# through ArrayValue::memory_first_*.

set -euo pipefail

target="subset_julia_vm/src/vm/exec/array_basic.rs"
errors=0

if rg -n 'ArrayData::[A-Za-z0-9_]+\(Vec::with_capacity' "$target"; then
    echo "ERROR: $target open-codes typed ArrayData capacity allocation."
    echo "       Use ArrayValue::memory_first_with_capacity for typed array builders."
    errors=$((errors + 1))
fi

if rg -n 'TypedArrayValue::new' "$target"; then
    echo "ERROR: $target constructs TypedArrayValue directly for array literals."
    echo "       Use ArrayValue::memory_first_with_capacity instead."
    errors=$((errors + 1))
fi

if rg -n -F 'ArrayValue::from_f64(Vec::with_capacity' "$target"; then
    echo "ERROR: $target constructs untyped F64 array literal builders directly."
    echo "       Use ArrayValue::memory_first_with_capacity instead."
    errors=$((errors + 1))
fi

if rg -n -F 'try_data_f64_mut()?.push' "$target"; then
    echo "ERROR: $target mutates untyped array literal builder storage directly."
    echo "       Use ArrayValue::push_f64 so builder growth stays behind ArrayValue mutation helpers."
    errors=$((errors + 1))
fi

if rg -n 'ArrayData::StructRefs' "$target"; then
    echo "ERROR: $target inspects StructRefs storage directly in array literal builders."
    echo "       Use ArrayValue helper methods for typed builder storage classification."
    errors=$((errors + 1))
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: Array literal Memory-first audit failed (Issue #3953)."
    exit 1
fi

echo "OK: array literal builders use Memory-first ArrayValue helpers (Issue #3953/#4018)."
