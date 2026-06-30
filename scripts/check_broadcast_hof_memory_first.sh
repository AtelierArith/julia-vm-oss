#!/usr/bin/env bash
# Issue #3962/#4019 - broadcast/HOF VM fallback result builders may still return
# the transitional ArrayValue container, but result storage must be materialized
# through Memory-first helpers.

set -euo pipefail

errors=0

targets=(
    "subset_julia_vm/src/vm/broadcast.rs"
    "subset_julia_vm/src/vm/exec/hof.rs"
    "subset_julia_vm/src/vm/hof_exec/dispatch.rs"
    "subset_julia_vm/src/vm/hof_exec/value_mode.rs"
    "subset_julia_vm/src/vm/util.rs"
)

storage_targets=(
    "subset_julia_vm/src/vm/broadcast.rs"
    "subset_julia_vm/src/vm/exec/hof.rs"
    "subset_julia_vm/src/vm/hof_exec/dispatch.rs"
    "subset_julia_vm/src/vm/hof_exec/value_mode.rs"
)

if rg -n 'ArrayValue::from_(f64|i64)\(' "${targets[@]}"; then
    echo "ERROR: broadcast/HOF result builders must use ArrayValue::memory_first_from_* helpers."
    errors=$((errors + 1))
fi

if rg -n 'ArrayData::(F64|I64|Any|StructRefs)' "${storage_targets[@]}"; then
    echo "ERROR: broadcast/HOF result builders must not open-code typed ArrayData result storage."
    errors=$((errors + 1))
fi

if rg -n 'TypedArrayValue::new|new_typed_array_ref' subset_julia_vm/src/vm/hof_exec/value_mode.rs; then
    echo "ERROR: HOF value-mode result builders must use ArrayValue::memory_first_* helpers."
    errors=$((errors + 1))
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: broadcast/HOF Memory-first audit failed (Issue #3962/#4019)."
    exit 1
fi

echo "OK: broadcast/HOF result builders use Memory-first helpers (Issue #3962/#4019)."
