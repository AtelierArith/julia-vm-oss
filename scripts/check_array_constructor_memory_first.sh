#!/usr/bin/env bash
# Issue #3952 - keep VM array constructor builtins on the Memory-first helper.
#
# Public array constructor builtins may still return the transitional
# Value::Array container, but they should allocate primitive Memory through
# ArrayValue::memory_first_* helpers instead of open-coding
# MemoryValue::undef_typed + ArrayValue::from_memory.

set -euo pipefail

targets=(
    "subset_julia_vm/src/vm/builtins_arrays.rs"
    "subset_julia_vm/src/vm/exec/array_basic.rs"
    "subset_julia_vm/src/vm/exec/rng.rs"
)
errors=0

for target in "${targets[@]}"; do
    if rg -n 'ArrayValue::from_memory(_with_override)?' "$target"; then
        echo "ERROR: $target directly wraps MemoryValue with ArrayValue::from_memory."
        echo "       Use ArrayValue::memory_first_* helpers for transitional Array allocation."
        errors=$((errors + 1))
    fi

    if rg -n 'ArrayValue::undef_typed\(' "$target"; then
        echo "ERROR: $target directly allocates typed undef arrays through ArrayValue::undef_typed."
        echo "       Use ArrayValue::memory_first_undef for typed constructor paths."
        errors=$((errors + 1))
    fi

    if rg -n 'mem\.fill\(' "$target"; then
        echo "ERROR: $target open-codes MemoryValue fill before Array wrapping."
        echo "       Use ArrayValue::memory_first_filled for filled constructor paths."
        errors=$((errors + 1))
    fi
done

if rg -n 'ArrayValue::from_f64\(' subset_julia_vm/src/vm/exec/rng.rs; then
    echo "ERROR: RNG array-producing instructions must use ArrayValue::memory_first_from_f64."
    errors=$((errors + 1))
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: Array constructor Memory-first audit failed (Issue #3952)."
    exit 1
fi

echo "OK: array constructor builtins use Memory-first ArrayValue helpers (Issue #3952)."
