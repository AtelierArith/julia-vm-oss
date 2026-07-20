# Checked integer arithmetic — subset of julia/base/checked.jl.
#
# Upstream wraps these in `module Checked` (imported into Base); sjulia loads
# Base files flat, so the functions are defined directly. Only the pieces
# needed by Base callers are ported (currently `checked_mul` /
# `mul_with_overflow` for Rational rem/mod, Issues #9416 / #9422).
#
# Upstream uses the `checked_smul_int` / `checked_umul_int` intrinsics; sjulia
# has no such intrinsics, so `mul_with_overflow` uses upstream's own pure-Julia
# fallbacks (the `BrokenSignedIntMul` Int128 path and the `BrokenUnsignedIntMul`
# UInt128 path in julia/base/checked.jl), which rely only on wrapping `*`,
# `fld`/`cld`, and `typemin`/`typemax` — valid for every fixed-width integer
# type, not just the 128-bit ones.

# Mixed-type promotion entry, mirroring upstream
# `checked_mul(x::Integer, y::Integer) = checked_mul(promote(x,y)...)`.
# Workaround: written in the two-variable form used by the promote fallbacks in
# base/promotion.jl instead of upstream's `checked_mul(promote(x, y)...)` splat
# form — the splatted call re-dispatches to this same method instead of the
# promoted same-type diagonal method, recursing forever (Issue #9513).
function checked_mul(x::Integer, y::Integer)
    px, py = promote(x, y)
    return checked_mul(px, py)
end

# Mixed-type promotion entries for add/sub, mirroring upstream
# `checked_add(x::Integer, y::Integer) = checked_add(promote(x,y)...)` and the
# analogous `checked_sub`. Written in the two-variable form (not upstream's
# splat) for the same anti-recursion reason as checked_mul above (Issue #9513).
function checked_add(x::Integer, y::Integer)
    px, py = promote(x, y)
    return checked_add(px, py)
end

function checked_sub(x::Integer, y::Integer)
    px, py = promote(x, y)
    return checked_sub(px, py)
end

# Mirrors upstream `throw_overflowerr_binaryop(op, x, y)`; the message must
# match upstream exactly (e.g. "4 * 340…455 overflowed for type UInt128").
function throw_overflowerr_binaryop(op, x, y)
    throw(OverflowError(string(x, " ", op, " ", y, " overflowed for type ", typeof(x))))
end

mul_with_overflow(x::Bool, y::Bool) = (x * y, false)

# Upstream's pure-Julia signed fallback (the `Int128 <: BrokenSignedIntMul`
# branch), generalized to every fixed-width Signed type. BigInt never reaches
# this: `checked_mul(x::BigInt, y::BigInt)` below is more specific.
function mul_with_overflow(x::T, y::T) where {T<:Signed}
    f = if y > 0
        # x * y > typemax(T) or x * y < typemin(T)
        x > fld(typemax(T), y) || x < cld(typemin(T), y)
    elseif y < 0
        # y == -1 can overflow fld
        x < cld(typemax(T), y) || y != -1 && x > fld(typemin(T), y)
    else
        false
    end
    return (x * y, f)
end

# Upstream's pure-Julia unsigned fallback (the `UInt128 <: BrokenUnsignedIntMul`
# branch), generalized to every fixed-width Unsigned type.
function mul_with_overflow(x::T, y::T) where {T<:Unsigned}
    # x * y > typemax(T)
    return (x * y, y > 0 && x > fld(typemax(T), y))
end

function checked_mul(x::T, y::T) where {T<:Integer}
    z, b = mul_with_overflow(x, y)
    b && throw_overflowerr_binaryop("*", x, y)
    return z
end

# BigInt is arbitrary precision — multiplication never overflows. Mirrors
# julia/base/gmp.jl `checked_mul(x::BigInt, y::BigInt) = x * y`.
checked_mul(x::BigInt, y::BigInt) = x * y

# -----------------------------------------------------------------------------
# checked_add / checked_sub (Issue #9527)
# -----------------------------------------------------------------------------
# Upstream uses the `checked_sadd_int` / `checked_uadd_int` (and ssub/usub)
# intrinsics; sjulia has no such intrinsics, so `add_with_overflow` /
# `sub_with_overflow` use upstream's own pure-Julia fallbacks (the
# `BrokenSignedInt` / `BrokenUnsignedInt` branches in julia/base/checked.jl),
# generalized to every fixed-width integer type. These rely only on wrapping
# `+`/`-`, sign comparisons, and bit-complement `~`.

add_with_overflow(x::Bool, y::Bool) = (x + y, false)
sub_with_overflow(x::Bool, y::Bool) = (x - y, false)

# Upstream's signed fallback (`BrokenSignedInt` branch): x and y have the same
# sign, and the result has a different sign.
function add_with_overflow(x::T, y::T) where {T<:Signed}
    r = x + y
    f = (x < 0) == (y < 0) != (r < 0)
    return (r, f)
end

# Upstream's unsigned fallback (`BrokenUnsignedInt` branch): x + y > typemax(T).
# Note: ~y == -y - 1, so x > ~y iff x + y wraps past typemax.
function add_with_overflow(x::T, y::T) where {T<:Unsigned}
    return (x + y, x > ~y)
end

# Upstream's signed fallback (`BrokenSignedInt` branch): x and y have different
# signs, and the result has a different sign than x.
function sub_with_overflow(x::T, y::T) where {T<:Signed}
    r = x - y
    f = (x < 0) != (y < 0) == (r < 0)
    return (r, f)
end

# Upstream's unsigned fallback (`BrokenUnsignedInt` branch): x - y < 0.
function sub_with_overflow(x::T, y::T) where {T<:Unsigned}
    return (x - y, x < y)
end

function checked_add(x::T, y::T) where {T<:Integer}
    z, b = add_with_overflow(x, y)
    b && throw_overflowerr_binaryop(:+, x, y)
    return z
end

function checked_sub(x::T, y::T) where {T<:Integer}
    z, b = sub_with_overflow(x, y)
    b && throw_overflowerr_binaryop(:-, x, y)
    return z
end

# BigInt is arbitrary precision — add/sub never overflow. Mirrors
# julia/base/gmp.jl `checked_add(a::BigInt, b::BigInt) = a + b` etc.
checked_add(x::BigInt, y::BigInt) = x + y
checked_sub(x::BigInt, y::BigInt) = x - y

# -----------------------------------------------------------------------------
# checked_neg / checked_abs (Issue #8812)
# -----------------------------------------------------------------------------
# Upstream julia/base/checked.jl. `checked_abs` is the overflow-checked |x|
# used by the generic same-type `lcm` in intfuncs.jl: two's complement signed
# integers cannot represent `abs(typemin(T))`, so that case throws instead of
# silently wrapping. Unsigned and Bool are identity; BigInt never overflows
# (julia/base/gmp.jl `checked_abs(x::BigInt) = abs(x)`).
function checked_neg(x::T) where {T<:Integer}
    return checked_sub(T(0), x)
end
checked_neg(x::BigInt) = -x

function checked_abs(x::T) where {T<:Signed}
    r = ifelse(x < 0, -x, x)
    r < 0 || return r
    throw(OverflowError(string("checked arithmetic: cannot compute |x| for x = ", x, "::", typeof(x))))
end
checked_abs(x::T) where {T<:Unsigned} = x
checked_abs(x::Bool) = x
checked_abs(x::BigInt) = abs(x)
