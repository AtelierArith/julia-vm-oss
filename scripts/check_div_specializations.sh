#!/usr/bin/env bash
# check_div_specializations.sh
#
# Issue #9381 — audit that subset_julia_vm/src/julia/base/int.jl uses
# upstream-shaped `BitSigned` / `BitUnsigned` same-type `div` methods instead
# of an explicit `div(x::TN, y::TN)` specialization for every signed and
# unsigned integer bit width.
#
# Background: bugs #3694 / #3696 / #3701 were all the same shape — the generic
# fallback `div(x, y) = floor(x / y)` widens through Float64 because `/` always
# returns Float64 for integer pairs. Issue #3699 originally pinned concrete
# per-width methods to prevent that drift. Issue #9381 retires that local
# duplication in favor of the upstream-style union methods while keeping this
# audit as a regression guard against reintroducing the concrete matrix.
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
# Exit code: 0 = OK, 1 = missing generic method(s) or concrete specialization(s)
# reintroduced.

set -euo pipefail

INT_JL="subset_julia_vm/src/julia/base/int.jl"

if [[ ! -f "$INT_JL" ]]; then
    echo "ERROR: $INT_JL not found. Run this script from the repository root."
    exit 1
fi

# Bit widths that must not regain a concrete same-type div specialization.
# Order intentional: signed narrow → wide, then unsigned narrow → wide. Mirrors
# the Julia primitive type tower in promotion.jl and the old Issue #3699 matrix.
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
for required in \
    "const BitSigned" \
    "const BitUnsigned" \
    "function div(x::T, y::T) where {T<:BitSigned}" \
    "function div(x::T, y::T) where {T<:BitUnsigned}"
do
    if ! grep -qF "$required" "$INT_JL"; then
        missing+=("$required")
    fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
    echo "ERROR: $INT_JL is missing the upstream-shaped generic div definitions:"
    for m in "${missing[@]}"; do
        echo "  - $m"
    done
    echo ""
    echo "Same-type fixed-width integer div must dispatch through BitSigned and"
    echo "BitUnsigned so the method table stays structural instead of width-by-width."
    echo "See Issue #9381."
    exit 1
fi

reintroduced=()
for w in "${WIDTHS[@]}"; do
    # Concrete same-type `div` methods are the old anti-pattern. The generic
    # BitSigned / BitUnsigned methods above preserve the type without duplicating
    # every width.
    if grep -qE "^function[[:space:]]+div\(x::${w},[[:space:]]*y::${w}\)" "$INT_JL"; then
        reintroduced+=("div(x::${w}, y::${w})")
    fi
done

if [[ ${#reintroduced[@]} -gt 0 ]]; then
    echo "ERROR: $INT_JL reintroduced concrete same-type div specializations:"
    for m in "${reintroduced[@]}"; do
        echo "  - function $m"
    done
    echo ""
    echo "Use the BitSigned / BitUnsigned parametric methods instead. This keeps"
    echo "the Issue #3699 type-preservation guard while satisfying Issue #9381."
    exit 1
fi

echo "OK: div uses BitSigned / BitUnsigned generic methods and no per-width same-type div specializations in $INT_JL (Issue #9381)."
