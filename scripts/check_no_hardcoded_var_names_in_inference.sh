#!/usr/bin/env bash
# check_no_hardcoded_var_names_in_inference.sh
#
# Check that type inference code in compile/expr/infer/ does not use hardcoded
# variable-name checks (name == "X") for struct-type constants.
#
# Such checks should use the global_types registry instead (Issue #3088, #3181).
#
# Exception: a small set of floating-point and integer special-value names are
# legitimate because they represent primitive types (F32/F64/I64), not struct
# instances, and are always defined regardless of prelude version:
#   NaN, Inf, NaN64, Inf64  — Float64 special values
#   NaN32, Inf32            — Float32 special values
#   ENDIAN_BOM              — byte-order marker (Int32/Int64)
#   op_name                 — operator name string (not a variable name)
#
# Usage: run from the repository root
#   bash scripts/check_no_hardcoded_var_names_in_inference.sh
#
# Exit code: 0 = within baseline, 1 = violation count increased

set -euo pipefail

INFERENCE_DIR="subset_julia_vm/src/compile/expr/infer"

if [[ ! -d "$INFERENCE_DIR" ]]; then
    echo "ERROR: $INFERENCE_DIR not found. Run this script from the repository root."
    exit 1
fi

# grep -v pattern for known-legitimate exceptions.
# All entries here are primitive types (not struct instances) or operator names.
ALLOWED_PATTERN='NaN\|Inf\|NaN32\|NaN64\|Inf32\|Inf64\|ENDIAN_BOM\|op_name'
BASELINE=9

hits=$(grep -rn 'name == "' "$INFERENCE_DIR" --include="*.rs" | grep -v "$ALLOWED_PATTERN" || true)
count=0
if [[ -n "$hits" ]]; then
    count=$(printf '%s\n' "$hits" | grep -c . || true)
fi

if [[ "$count" -gt 0 ]]; then
    echo "Hardcoded variable name(s) found in type inference code: $count / $BASELINE baseline."
    echo "Use the global_types registry instead of hardcoded name == \"X\" checks."
    echo "See Issue #3088 and #3181 for background. Offending lines:"
    echo "$hits"
fi

if [[ "$count" -gt "$BASELINE" ]]; then
    echo ""
    echo "ERROR: Count ($count) exceeds baseline ($BASELINE). New hardcoded inference names added."
    exit 1
fi

echo "OK: hardcoded inference variable-name count is within baseline (Issue #3088, #3181)."
