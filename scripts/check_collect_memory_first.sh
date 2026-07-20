#!/usr/bin/env bash
# Issue #3954 - keep collect materialization on Memory-first ArrayValue helpers.
#
# collect(range), collect(tuple), collect(array), and eager collect(generator)
# may still return the transitional Value::Array container, but typed result
# storage should be materialized through MemoryValue before the Array wrapper is
# created. collect(::String) now routes through Pure Julia dispatch; keep this
# audit blocking direct Char ArrayData materialization from returning.

set -euo pipefail

errors=0

# Range::collect moved to the bytecode crate in the #8655/#8656 crate split;
# the old vm/value/range.rs path made this audit's collect(range) check a
# silent no-op (Issue #9573).
range_target="subset_julia_vm_bytecode/src/value/range.rs"
legacy_range_target="subset_julia_vm_vm/src/vm/exec/range.rs"
iteration_target="subset_julia_vm_vm/src/vm/type_ops/iteration.rs"
iterators_jl_target="subset_julia_vm/src/julia/base/iterators.jl"

# Guard against path drift (Issue #9573): a grep over a missing file is a
# silently dead check ("OK" without looking). Fail loudly instead.
for target in "$range_target" "$legacy_range_target" "$iteration_target" "$iterators_jl_target"; do
    if [[ ! -f "$target" ]]; then
        echo "ERROR: audit target file missing: $target (moved/removed by a refactor? Repoint this audit — Issue #9573)."
        errors=$((errors + 1))
    fi
done
if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: collect Memory-first audit failed (Issue #3954)."
    exit 1
fi

if rg -n 'ArrayValue::(i64_vector|vector|from_i64|from_f64)\(' "$range_target"; then
    echo "ERROR: $range_target materializes collect(range) without Memory-first helpers."
    errors=$((errors + 1))
fi

if rg -n 'ArrayValue::(from_i64|from_f64)\(' "$legacy_range_target"; then
    echo "ERROR: $legacy_range_target materializes legacy eager ranges without Memory-first helpers."
    errors=$((errors + 1))
fi

if rg -n 'ArrayValue::(from_i64|from_f64)\(' "$iteration_target"; then
    echo "ERROR: $iteration_target materializes collect(tuple) without Memory-first helpers."
    errors=$((errors + 1))
fi

if rg -n 'ArrayData::Char\(' "$iteration_target"; then
    echo "ERROR: $iteration_target materializes collect(string) without Memory-first helpers."
    errors=$((errors + 1))
fi

if rg -n 'borrowed\.data\.clone\(\)' "$iteration_target"; then
    echo "ERROR: $iteration_target materializes collect(array) by cloning ArrayData directly."
    errors=$((errors + 1))
fi

if rg -n 'Full lazy generator support requires|return the underlying iterator collected' "$iteration_target"; then
    echo "ERROR: $iteration_target silently collects lazy Generator inner values without applying f(x)."
    errors=$((errors + 1))
fi

if awk '
    /elseif n == 2/ { in_2d = 1; next }
    in_2d && /elseif n == 3/ { in_2d = 0 }
    in_2d && /Array\{T\}\(undef/ { print FILENAME ":" FNR ":" $0; found = 1 }
    END { exit found ? 0 : 1 }
' "$iterators_jl_target"; then
    echo "ERROR: $iterators_jl_target::_array_for_inner_shape 2-D path bypasses similar(Array{T}, dims)."
    errors=$((errors + 1))
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: collect Memory-first audit failed (Issue #3954)."
    exit 1
fi

echo "OK: collect result materialization uses Memory-first ArrayValue helpers (Issue #3954)."
