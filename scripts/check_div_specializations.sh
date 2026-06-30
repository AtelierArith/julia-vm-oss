#!/usr/bin/env bash
# check_div_specializations.sh
#
# Issue #3699 — audit that subset_julia_vm/src/julia/base/int.jl carries an
# explicit `div(x::TN, y::TN)` specialization for every signed and unsigned
# integer bit width supported by the VM (Int8/16/32/64/128, UInt8/16/32/64/128).
#
# Background: bugs #3694 / #3696 / #3701 were all the same shape — the generic
# fallback `div(x, y) = floor(x / y)` widens through Float64 because `/` always
# returns Float64 for integer pairs. Without a per-width specialization the
# return type drifts. Catching the missing arm at lint time stops the next
# incarnation of that bug before it ships.
#
# Companion fixtures:
#   subset_julia_vm/tests/fixtures/type_preservation/*_arith_matrix.jl
#
# Scope: only `div` is checked here, because that is the operator the recurring
# bug cluster pointed at. The other operators called out in the Issue (+, -, *,
# ==, <, <=, >, >=) flow through different layers — runtime intrinsics with
# baked-in type preservation for narrow ints (PR #3565), the I128 / U128
# early-routes in compile/expr/binary/mod.rs (Issues #3621 / #3697), and the
# runtime cmp early-route from Issue #3696. Those layers are audited by the
# fixture matrix at runtime; adding pure-Julia method specializations for them
# would be redundant with the intrinsic-level type preservation.
#
# Usage: run from the repository root
#   bash scripts/check_div_specializations.sh
#
# Exit code: 0 = OK, 1 = missing specialization(s).

set -euo pipefail

INT_JL="subset_julia_vm/src/julia/base/int.jl"

if [[ ! -f "$INT_JL" ]]; then
    echo "ERROR: $INT_JL not found. Run this script from the repository root."
    exit 1
fi

# Bit widths that must have a div specialization. Order intentional: signed
# narrow → wide, then unsigned narrow → wide. Mirrors the Julia primitive type
# tower in promotion.jl and the `Int64`-anchored doc in NUMERIC_TYPES.md.
WIDTHS=(
    "Int8"
    "Int16"
    "Int32"
    "Int64"
    "Int128"
    "UInt8"
    "UInt16"
    "UInt32"
    "UInt64"
    "UInt128"
)

missing=()

for w in "${WIDTHS[@]}"; do
    # Each line of int.jl that defines `div` for this width. We require the
    # canonical `function div(x::TN, y::TN)` shape (whitespace tolerant).
    if ! grep -qE "^function div\(x::${w}, y::${w}\)" "$INT_JL"; then
        missing+=("div(x::${w}, y::${w})")
    fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
    echo "ERROR: $INT_JL is missing the following div specializations:"
    for m in "${missing[@]}"; do
        echo "  - function $m"
    done
    echo ""
    echo "Each integer bit width must have its own div(x::TN, y::TN) so that"
    echo "the result preserves TN. The generic div(x, y) = floor(x / y) widens"
    echo "through Float64 because '/' on integers always returns Float64."
    echo "See Issues #3694, #3696, #3701 and docs/vm/TYPE_PRESERVATION.md."
    exit 1
fi

echo "OK: all signed/unsigned int widths have div specializations in $INT_JL (Issue #3699)."
