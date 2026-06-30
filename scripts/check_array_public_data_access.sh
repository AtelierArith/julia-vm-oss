#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail=0

echo "Checking public ArrayValue data-access audit (Issue #3927/#4018)..."

broadcast_file="$ROOT/subset_julia_vm/src/vm/broadcast.rs"
while IFS= read -r match; do
    case "$match" in
        *"src_idx * 2]"*|*"src_idx * 2 + 1]"*)
            ;;
        *)
            echo "ERROR: broadcast.rs real-valued public broadcast must use get_linear_f64, not direct try_data_f64:"
            echo "  $match"
            fail=1
            ;;
    esac
done < <(rg -n "try_data_f64\\(\\)\\?" "$broadcast_file" || true)

if ! rg -q "get_linear_f64" "$broadcast_file"; then
    echo "ERROR: broadcast.rs must route real-valued public broadcast through get_linear_f64"
    fail=1
fi

array_access_file="$ROOT/subset_julia_vm/src/vm/value/array_value/access.rs"
if ! rg -q "if let Some\\(parent\\) = &self\\.shared_parent" "$array_access_file"; then
    echo "ERROR: ArrayValue logical reads must classify reshape shared-parent arrays before raw data access"
    fail=1
fi
if ! rg -q "to_logical_value_vec|to_logical_f64_vec" "$array_access_file"; then
    echo "ERROR: ArrayValue must expose logical element vector reads for shared-parent arrays"
    fail=1
fi

array_index_file="$ROOT/subset_julia_vm/src/vm/exec/array_index.rs"
if ! rg -q "memory_first_slice_from_values" "$array_index_file"; then
    echo "ERROR: create_sliced_array must allocate slice results through the Memory-first helper"
    fail=1
fi
if rg -n "ArrayValue::new|TypedArrayValue::new" "$array_index_file"; then
    echo "ERROR: array_index.rs must not directly construct sliced ArrayValue results"
    fail=1
fi
if rg -n "data\\.get_value" "$array_index_file"; then
    echo "ERROR: array_index.rs public/indexing paths must use logical ArrayValue reads, not raw data.get_value"
    fail=1
fi
if rg -n "ArrayData::StructRefs" "$array_index_file"; then
    echo "ERROR: array_index.rs public/indexing paths must use ArrayValue helpers, not raw StructRefs storage"
    fail=1
fi

builtins_arrays_file="$ROOT/subset_julia_vm/src/vm/builtins_arrays.rs"
if rg -n "ArrayData::StructRefs" "$builtins_arrays_file"; then
    echo "ERROR: builtins_arrays.rs public Array mutation must use ArrayValue helper methods, not raw StructRefs storage"
    fail=1
fi

struct_ops_file="$ROOT/subset_julia_vm/src/vm/exec/struct_ops.rs"
if rg -n "arr_borrow\\.data\\.get_value" "$struct_ops_file"; then
    echo "ERROR: struct_ops.rs Array splat paths must use logical ArrayValue reads, not raw data.get_value"
    fail=1
fi

builtins_macro_file="$ROOT/subset_julia_vm/src/vm/builtins_macro/mod.rs"
if rg -n "borrowed\\.data\\.get_value" "$builtins_macro_file"; then
    echo "ERROR: builtins_macro Expr splat paths must use logical ArrayValue reads, not raw data.get_value"
    fail=1
fi

binary_both_file="$ROOT/subset_julia_vm/src/vm/exec/binary_both.rs"
if rg -n "memory\\.data\\.get_value" "$binary_both_file"; then
    echo "ERROR: binary_both.rs Memory<->Array equality must use the Memory public 1-indexed get(), not raw memory.data.get_value"
    fail=1
fi

dynamic_dispatch_file="$ROOT/subset_julia_vm/src/vm/dynamic_ops/dispatch.rs"
if rg -n "ArrayData::(StructRefs|Any)" "$dynamic_dispatch_file"; then
    echo "ERROR: dynamic_ops/dispatch.rs must use ArrayValue storage helpers, not raw ArrayData tags"
    fail=1
fi
if ! rg -q "supports_inline_dynamic_storage" "$dynamic_dispatch_file"; then
    echo "ERROR: dynamic_ops/dispatch.rs must classify inline Array ops through ArrayValue helper"
    fail=1
fi

if [[ "$fail" -ne 0 ]]; then
    exit 1
fi

echo "OK: public ArrayValue data-access audit passed."
