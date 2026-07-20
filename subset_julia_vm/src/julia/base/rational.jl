# =============================================================================
# Rational - Rational number type
# =============================================================================
# Based on Julia's base/rational.jl
#
# Rational numbers are represented as num//den where num and den are integers.
# The representation is always normalized (reduced to lowest terms with positive denominator).
#
# IMPORTANT: This module uses Julia-standard operator overloading.
# All arithmetic uses Base.:+ style extensions.
#
# Design: Inner constructor is raw (no normalization, like Julia's unsafe_rational).
# Outer constructors perform GCD normalization using concrete types to avoid
# Issue #2384 (where T dispatch in inner constructor returns Float64 from div).

# Workaround: keep raw allocation in a same-name marker-token inner until differently named struct-body helpers can use `new` (Issue #11005).
struct _RationalRawToken end

# Rational number struct (parametric version)
# Julia's Rational is Rational{T<:Integer} <: Real
struct Rational{T<:Integer} <: Real
    num::T
    den::T
    # Inner constructor: raw, no normalization (unsafe_rational equivalent)
    function Rational{T}(::_RationalRawToken, num::T, den::T) where T
        return new{T}(num, den)
    end
end

# =============================================================================
# Raw terminal constructor (Issue #5132)
# =============================================================================
# Mirror of Base.unsafe_rational: an unexported, no-normalization terminal.
# It reaches the tagged explicit-parametric inner with the caller's `T` binding;
# the private marker argument makes that raw three-argument method distinct from
# every public two-argument normalizing outer constructor. The explicit-typed
# public constructors below delegate here for final allocation without recursion.
function unsafe_rational(::Type{T}, num::T, den::T) where {T<:Integer}
    return Rational{T}(_RationalRawToken(), num, den)
end

# =============================================================================
# 0//0 rejection (Issue #9514)
# =============================================================================
# Upstream `julia/base/rational.jl` rejects the invalid rational `0//0` in
# `Rational{T}(num, den)` via
#   iszero(den) && iszero(num) && __throw_rational_argerror_zero(T)
# The `1//0` / `-1//0` Inf sentinels (num != 0, den == 0) are still permitted.
# sjulia's constructors leave den == 0 raw to preserve those sentinels, so this
# helper is invoked from each den == 0 branch guarded by an additional
# `num == 0` check, matching upstream semantics and message. Without it, the
# Rational rem/mod path `x - div(x, y) * y` with an infinite divisor `y`
# produced the invalid `0//0` instead of raising an ArgumentError.
@noinline function __throw_rational_argerror_zero(::Type{T}) where {T}
    throw(ArgumentError(string("invalid rational: zero(", T, ")//zero(", T, ")")))
end

# =============================================================================
# Explicit type-parameter constructors (Issue #5132)
# =============================================================================
# `Rational{T}(num, den)` must coerce both fields to the requested element type
# `T` and normalize (gcd reduction + positive denominator), matching upstream
# `julia/base/rational.jl`. Without these, `Rational{Int8}(6, 4)` would call the
# raw inner constructor, infer T from the Int64 arguments, and skip reduction
# (yielding `Rational{Int64}` `6//4` instead of `Rational{Int8}` `3//2`).
#
# A den == 0 input is left raw to preserve the Inf/NaN sentinels (e.g. 1//0,
# 0//0) that the rest of this file relies on, mirroring the existing
# `Rational(IntN, IntN)` outer constructors.

# Int64
function Rational{Int64}(num::Integer, den::Integer)
    n = Int64(num)
    d = Int64(den)
    if d == Int64(0)
        n == Int64(0) && __throw_rational_argerror_zero(Int64)
        return unsafe_rational(Int64, n, d)
    end
    if d < Int64(0)
        n = Int64(0) - n
        d = Int64(0) - d
    end
    g = gcd(n, d)
    if g > Int64(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(Int64, n, d)
end

# Int32
function Rational{Int32}(num::Integer, den::Integer)
    n = Int32(num)
    d = Int32(den)
    if d == Int32(0)
        n == Int32(0) && __throw_rational_argerror_zero(Int32)
        return unsafe_rational(Int32, n, d)
    end
    if d < Int32(0)
        n = Int32(0) - n
        d = Int32(0) - d
    end
    g = gcd(n, d)
    if g > Int32(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(Int32, n, d)
end

# Int16
function Rational{Int16}(num::Integer, den::Integer)
    n = Int16(num)
    d = Int16(den)
    if d == Int16(0)
        n == Int16(0) && __throw_rational_argerror_zero(Int16)
        return unsafe_rational(Int16, n, d)
    end
    if d < Int16(0)
        n = Int16(0) - n
        d = Int16(0) - d
    end
    g = gcd(n, d)
    if g > Int16(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(Int16, n, d)
end

# Int8
function Rational{Int8}(num::Integer, den::Integer)
    n = Int8(num)
    d = Int8(den)
    if d == Int8(0)
        n == Int8(0) && __throw_rational_argerror_zero(Int8)
        return unsafe_rational(Int8, n, d)
    end
    if d < Int8(0)
        n = Int8(0) - n
        d = Int8(0) - d
    end
    g = gcd(n, d)
    if g > Int8(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(Int8, n, d)
end

# BigInt
function Rational{BigInt}(num::Integer, den::Integer)
    n = big(num)
    d = big(den)
    if d == big(0)
        n == big(0) && __throw_rational_argerror_zero(BigInt)
        return unsafe_rational(BigInt, n, d)
    end
    if d < big(0)
        n = big(0) - n
        d = big(0) - d
    end
    g = gcd(n, d)
    if g > big(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(BigInt, n, d)
end

# Concrete two-argument constructors for the wide/unsigned element types. Like
# the Int8..Int64/BigInt methods above these are enumerated because the generic
# `where {T<:Integer}` form below is not reliably selected by the VM dispatcher
# for a syntactically-parameterized call with narrower Integer arguments (e.g.
# `Rational{Int128}(5, 1)` fell through to the outer `Rational(::Int64,::Int64)`
# and produced Int64 fields instead of coercing to Int128, Issue #9526). Int128
# flips the sign like the signed forms; the Unsigned family and Bool never do
# (negating them would wrap/error). den == 0 is left raw to keep Inf/NaN
# sentinels, matching the constructors above.
function Rational{Int128}(num::Integer, den::Integer)
    n = Int128(num)
    d = Int128(den)
    if d == Int128(0)
        n == Int128(0) && __throw_rational_argerror_zero(Int128)
        return unsafe_rational(Int128, n, d)
    end
    if d < Int128(0)
        n = Int128(0) - n
        d = Int128(0) - d
    end
    g = gcd(n, d)
    if g > Int128(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(Int128, n, d)
end
function Rational{UInt8}(num::Integer, den::Integer)
    n = UInt8(num)
    d = UInt8(den)
    if d == UInt8(0)
        n == UInt8(0) && __throw_rational_argerror_zero(UInt8)
        return unsafe_rational(UInt8, n, d)
    end
    g = gcd(n, d)
    if g > UInt8(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(UInt8, n, d)
end
function Rational{UInt16}(num::Integer, den::Integer)
    n = UInt16(num)
    d = UInt16(den)
    if d == UInt16(0)
        n == UInt16(0) && __throw_rational_argerror_zero(UInt16)
        return unsafe_rational(UInt16, n, d)
    end
    g = gcd(n, d)
    if g > UInt16(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(UInt16, n, d)
end
function Rational{UInt32}(num::Integer, den::Integer)
    n = UInt32(num)
    d = UInt32(den)
    if d == UInt32(0)
        n == UInt32(0) && __throw_rational_argerror_zero(UInt32)
        return unsafe_rational(UInt32, n, d)
    end
    g = gcd(n, d)
    if g > UInt32(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(UInt32, n, d)
end
function Rational{UInt64}(num::Integer, den::Integer)
    n = UInt64(num)
    d = UInt64(den)
    if d == UInt64(0)
        n == UInt64(0) && __throw_rational_argerror_zero(UInt64)
        return unsafe_rational(UInt64, n, d)
    end
    g = gcd(n, d)
    if g > UInt64(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(UInt64, n, d)
end
function Rational{UInt128}(num::Integer, den::Integer)
    n = UInt128(num)
    d = UInt128(den)
    if d == UInt128(0)
        n == UInt128(0) && __throw_rational_argerror_zero(UInt128)
        return unsafe_rational(UInt128, n, d)
    end
    g = gcd(n, d)
    if g > UInt128(1)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(UInt128, n, d)
end
function Rational{Bool}(num::Integer, den::Integer)
    n = Bool(num)
    d = Bool(den)
    if !d
        n || __throw_rational_argerror_zero(Bool)
    end
    return unsafe_rational(Bool, n, d)
end

# Generic typed constructor for the remaining integer element types. The
# explicit Int8..Int64/BigInt and Int128/Unsigned/Bool methods above are more
# specific and still win for those types; this method only fires for element
# types that have no dedicated constructor. It mirrors upstream
# `Rational{T}(num, den) where T<:Integer` (base/rational.jl): reduce by gcd and
# normalize the sign, but only flip the sign for Signed element types — Bool and
# Unsigned values are never negative, and negating them would wrap/error.
# A den == 0 input is left raw to preserve the Inf/NaN sentinels, matching the
# signed constructors above (Issue #9315).
function Rational{T}(num::Integer, den::Integer) where {T<:Integer}
    n = T(num)
    d = T(den)
    if d == zero(T)
        n == zero(T) && __throw_rational_argerror_zero(T)
        return unsafe_rational(T, n, d)
    end
    if T <: Signed && signbit(d)
        n = zero(T) - n
        d = zero(T) - d
    end
    g = gcd(n, d)
    if g > one(T)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(T, n, d)
end

# Single-argument form: Rational{T}(x) == Rational{T}(x, 1)
function Rational{Int64}(x::Integer)
    return unsafe_rational(Int64, Int64(x), Int64(1))
end
function Rational{Int32}(x::Integer)
    return unsafe_rational(Int32, Int32(x), Int32(1))
end
function Rational{Int16}(x::Integer)
    return unsafe_rational(Int16, Int16(x), Int16(1))
end
function Rational{Int8}(x::Integer)
    return unsafe_rational(Int8, Int8(x), Int8(1))
end
function Rational{BigInt}(x::Integer)
    return unsafe_rational(BigInt, big(x), big(1))
end
# Single-argument Integer constructors for the wide/unsigned element types.
# Like the signed forms above they are enumerated with concrete element types
# (never a generic `where T`, see the NOTE below / Issue #9315) and coexist with
# the `Rational{T}(x::Rational)` conversions further down exactly as the Int64
# pair does. Needed so that promoting an Integer to Rational{Int128}/Rational{UInt*}
# (the second operand of `max(3//4, Int128(5))`) can build the widened value via
# the generic `convert(::Type{T}, x) = T(x)` fallback (Issue #9526).
function Rational{Int128}(x::Integer)
    return unsafe_rational(Int128, Int128(x), Int128(1))
end
function Rational{UInt8}(x::Integer)
    return unsafe_rational(UInt8, UInt8(x), UInt8(1))
end
function Rational{UInt16}(x::Integer)
    return unsafe_rational(UInt16, UInt16(x), UInt16(1))
end
function Rational{UInt32}(x::Integer)
    return unsafe_rational(UInt32, UInt32(x), UInt32(1))
end
function Rational{UInt64}(x::Integer)
    return unsafe_rational(UInt64, UInt64(x), UInt64(1))
end
function Rational{UInt128}(x::Integer)
    return unsafe_rational(UInt128, UInt128(x), UInt128(1))
end
function Rational{Bool}(x::Integer)
    return unsafe_rational(Bool, Bool(x), Bool(1))
end
# NOTE: no generic single-argument `Rational{T}(x::Integer) where T` is defined
# on purpose (Issue #9315). Under sjulia's typed-constructor dispatch, the bare
# abstract `x::Integer` annotation was loosely matched to a `Rational` argument,
# stealing the `Rational{Int64}(x::Rational)` conversion call above and raising
# `InexactError: Int64(3//4)`. The single-argument OUTER constructor
# `Rational(num::T)` below reaches the raw 2-argument path instead, so Bool /
# Unsigned / Int128 single-argument construction still works.

# Rational-from-Rational conversion: Rational{T}(r) re-types num/den to T.
# r is already normalized, so no further reduction is required.
function Rational{Int64}(x::Rational)
    return unsafe_rational(Int64, Int64(x.num), Int64(x.den))
end
function Rational{Int32}(x::Rational)
    return unsafe_rational(Int32, Int32(x.num), Int32(x.den))
end
function Rational{Int16}(x::Rational)
    return unsafe_rational(Int16, Int16(x.num), Int16(x.den))
end
function Rational{Int8}(x::Rational)
    return unsafe_rational(Int8, Int8(x.num), Int8(x.den))
end
function Rational{BigInt}(x::Rational)
    return unsafe_rational(BigInt, big(x.num), big(x.den))
end
# Rational-from-Rational conversion for the wide/unsigned element types. These
# mirror the signed constructors above and, like them, are enumerated with
# concrete element types on purpose: a generic single-argument
# `Rational{T}(x::Rational) where {T<:Integer}` would be loosely matched to
# Integer arguments by the VM dispatcher (same hazard as the single-argument
# Integer form, see the note above / Issue #9315). x is already normalized, so
# no further gcd reduction is required. Needed so that promoting a
# Rational{Int64} to Rational{Int128}/Rational{UInt*} (e.g. via the max/min
# promote fallback `max(3//4, Int128(5))`) can construct the widened value
# instead of raising MethodError (Issue #9526).
function Rational{Int128}(x::Rational)
    return unsafe_rational(Int128, Int128(x.num), Int128(x.den))
end
function Rational{UInt8}(x::Rational)
    return unsafe_rational(UInt8, UInt8(x.num), UInt8(x.den))
end
function Rational{UInt16}(x::Rational)
    return unsafe_rational(UInt16, UInt16(x.num), UInt16(x.den))
end
function Rational{UInt32}(x::Rational)
    return unsafe_rational(UInt32, UInt32(x.num), UInt32(x.den))
end
function Rational{UInt64}(x::Rational)
    return unsafe_rational(UInt64, UInt64(x.num), UInt64(x.den))
end
function Rational{UInt128}(x::Rational)
    return unsafe_rational(UInt128, UInt128(x.num), UInt128(x.den))
end
function Rational{Bool}(x::Rational)
    return unsafe_rational(Bool, Bool(x.num), Bool(x.den))
end

# =============================================================================
# Outer constructors with GCD normalization
# =============================================================================

# Single-argument constructors
function Rational(num::Int64)
    return Rational{Int64}(num, Int64(1))
end

function Rational(num::Int32)
    return Rational{Int32}(num, Int32(1))
end

function Rational(num::Int16)
    return Rational{Int16}(num, Int16(1))
end

function Rational(num::Int8)
    return Rational{Int8}(num, Int8(1))
end

function Rational(num::BigInt)
    return Rational{BigInt}(num, big(1))
end

# Same-type outer constructor for integer element types. Upstream writes this as
# `Rational(n::T, d::T) where {T<:Integer} = Rational{T}(n, d)`.
#
# This is the method that TERMINATES the promote-based `Rational(::Integer,
# ::Integer)` fallback below for same-type pairs such as `true // true` and
# `0x01 // 0x03` (Issue #9315). Without it, `promote(true, true)` returns the
# unchanged `(true, true)` pair, dispatch re-enters the promoting fallback, and
# the constructor self-recurses until MAX_CALL_DEPTH (the classic
# promote-fallback recursion trap, Issue #5966). Being parameterized on a single
# type variable `T`, it is more specific than the two-`::Integer` fallback, and
# The reduction is inlined here (rather than delegating to `Rational{T}(...)`)
# because a `Rational{T}(...)` call under a type *variable* `T` routes to the
# raw inner constructor — the same bare-name path `unsafe_rational` uses — and
# would skip normalization, mirroring the signed outer/typed constructors above
# which likewise duplicate the reduction. Sign is only flipped for Signed `T`;
# Bool/Unsigned values are never negative.
function Rational(num::T, den::T) where {T<:Integer}
    n = num
    d = den
    if d == zero(T)
        n == zero(T) && __throw_rational_argerror_zero(T)
        return unsafe_rational(T, n, d)
    end
    if T <: Signed && signbit(d)
        n = zero(T) - n
        d = zero(T) - d
    end
    g = gcd(n, d)
    if g > one(T)
        n = div(n, g)
        d = div(d, g)
    end
    return unsafe_rational(T, n, d)
end

# NOTE: no generic single-argument `Rational(num::T) where T<:Integer` is
# defined on purpose (Issue #9315). Like the single-argument typed constructor
# above, sjulia's dispatcher loosely matched its parametric integer argument to
# a `Rational` value, so `Rational(3 // 4)` was routed here (raising an internal
# LoadSlot error) instead of to a `Rational`-to-`Rational` path. Single-argument
# construction for Bool / Unsigned / Int128 is out of scope for this fix, which
# targets the `//` (two-argument) recursion; the explicit signed single-argument
# constructors above are unaffected.

# Identity constructor: a Rational is already a Rational (Issue #9363).
# Upstream: base/rational.jl `Rational(x::Rational) = x`. Without this method a
# `Rational` argument was loose-matched to the concrete single-argument
# `Rational(num::Int64)` constructor above, whose body treated the struct value
# as an integer numerator and raised an InternalError.
Rational(x::Rational) = x

# Mixed-type constructor: promote both args to a common type
function Rational(num::Integer, den::Integer)
    pn, pd = promote(num, den)
    return Rational(pn, pd)
end

# =============================================================================
# // operator: creates Rational from two integers
# =============================================================================
# Based on Julia's base/rational.jl:91: //(n::Integer, d::Integer) = Rational(n,d)
function //(n::Integer, d::Integer)
    return Rational(n, d)
end

# Rational-over-rational exact division. Upstream defines this in
# base/rational.jl and specializes Rational{BigInt} in base/gmp.jl; sjulia's
# existing Rational `/` path already normalizes through Rational(num, den).
function //(x::Rational, y::Rational)
    return x / y
end

function //(x::Rational{BigInt}, y::Rational{BigInt})
    return x / y
end

# =============================================================================
# Accessor functions (Julia standard)
# =============================================================================

function numerator(x::Rational)
    return x.num
end

function denominator(x::Rational)
    return x.den
end

# Rational{BigInt} accessor specializations (Issue #2497)
function numerator(x::Rational{BigInt})
    return x.num
end

function denominator(x::Rational{BigInt})
    return x.den
end

# =============================================================================
# Type conversion (Julia standard)
# =============================================================================

function float(x::Rational)
    return x.num / x.den
end

# =============================================================================
# Predicates (Julia standard - using dispatch)
# =============================================================================

function iszero(x::Rational)
    return iszero(numerator(x))
end

function isone(x::Rational)
    return isone(numerator(x)) & isone(denominator(x))
end

function isinteger(x::Rational)
    return x.den == 1
end

# signbit for Rational: check if the numerator is negative
# Based on Julia's base/rational.jl:365
function signbit(x::Rational)
    return x.num < 0
end

# =============================================================================
# Unary operators (Julia standard)
# =============================================================================

# Negation: -x
function Base.:-(x::Rational)
    return Rational(zero(x.num) - x.num, x.den)
end

# Inverse: inv(x) = 1/x
function inv(x::Rational)
    if x.num == 0
        return Rational(one(x.den), zero(x.den))
    end
    return Rational(x.den, x.num)
end

# Rational non-Int64 unary specializations (Issue #2497)
function Base.:-(x::Rational{Int32})
    return Rational{Int32}(Int32(0) - x.num, x.den)
end

function inv(x::Rational{Int32})
    if x.num == Int32(0)
        return Rational{Int32}(Int32(1), Int32(0))
    end
    return Rational{Int32}(x.den, x.num)
end

function Base.:-(x::Rational{Int16})
    return Rational{Int16}(Int16(0) - x.num, x.den)
end

function inv(x::Rational{Int16})
    if x.num == Int16(0)
        return Rational{Int16}(Int16(1), Int16(0))
    end
    return Rational{Int16}(x.den, x.num)
end

function Base.:-(x::Rational{Int8})
    return Rational{Int8}(Int8(0) - x.num, x.den)
end

function inv(x::Rational{Int8})
    if x.num == Int8(0)
        return Rational{Int8}(Int8(1), Int8(0))
    end
    return Rational{Int8}(x.den, x.num)
end

function Base.:-(x::Rational{BigInt})
    return Rational(big(0) - x.num, x.den)
end

function inv(x::Rational{BigInt})
    if x.num == big(0)
        return Rational(big(1), big(0))
    end
    return Rational(x.den, x.num)
end

# =============================================================================
# Binary arithmetic operators (Julia standard)
# =============================================================================

# Addition: x + y
function Base.:+(x::Rational, y::Rational)
    num = x.num * y.den + y.num * x.den
    den = x.den * y.den
    return Rational(num, den)
end

# Subtraction: x - y
function Base.:-(x::Rational, y::Rational)
    num = x.num * y.den - y.num * x.den
    den = x.den * y.den
    return Rational(num, den)
end

# Multiplication: x * y
function Base.:*(x::Rational, y::Rational)
    num = x.num * y.num
    den = x.den * y.den
    return Rational(num, den)
end

# Division: x / y
function Base.:/(x::Rational, y::Rational)
    num = x.num * y.den
    den = x.den * y.num
    return Rational(num, den)
end

# Mixed Rational/Integer arithmetic mirrors Julia's base/rational.jl direct
# methods, keeping these operations out of the broad Number promotion fallback.
function Base.:+(x::Rational, y::BigInt)
    return convert(Rational{BigInt}, x) + Rational{BigInt}(y, big(1))
end

function Base.:+(y::BigInt, x::Rational)
    return Rational{BigInt}(y, big(1)) + convert(Rational{BigInt}, x)
end

# Mirror upstream julia/base/rational.jl: the cross-multiplied numerator is
# formed with checked_mul/checked_add so an over/underflow raises a catchable
# OverflowError instead of silently wrapping (Issue #9527).
function Base.:+(x::Rational, y::Integer)
    return Rational(checked_add(x.num, checked_mul(x.den, y)), x.den)
end

function Base.:+(y::Integer, x::Rational)
    return Rational(checked_add(checked_mul(x.den, y), x.num), x.den)
end

function Base.:-(x::Rational, y::BigInt)
    return convert(Rational{BigInt}, x) - Rational{BigInt}(y, big(1))
end

function Base.:-(y::BigInt, x::Rational)
    return Rational{BigInt}(y, big(1)) - convert(Rational{BigInt}, x)
end

function Base.:-(x::Rational, y::Integer)
    return Rational(checked_sub(x.num, checked_mul(x.den, y)), x.den)
end

function Base.:-(y::Integer, x::Rational)
    return Rational(checked_sub(checked_mul(x.den, y), x.num), x.den)
end

function Base.:*(x::Rational, y::BigInt)
    return convert(Rational{BigInt}, x) * Rational{BigInt}(y, big(1))
end

function Base.:*(y::BigInt, x::Rational)
    return Rational{BigInt}(y, big(1)) * convert(Rational{BigInt}, x)
end

function Base.:*(x::Rational, y::Integer)
    return Rational(x.num * y, x.den)
end

function Base.:*(y::Integer, x::Rational)
    return Rational(y * x.num, x.den)
end

function Base.:/(x::Rational, y::BigInt)
    return convert(Rational{BigInt}, x) / Rational{BigInt}(y, big(1))
end

function Base.:/(y::BigInt, x::Rational)
    return Rational{BigInt}(y, big(1)) / convert(Rational{BigInt}, x)
end

function Base.:/(x::Rational, y::Integer)
    return Rational(x.num, x.den * y)
end

function Base.:/(y::Integer, x::Rational)
    return Rational(y * x.den, x.num)
end

# Mixed Rational/AbstractFloat arithmetic (Issue #9524).
#
# Upstream reaches these through the generic `Number` promotion fallback
# (`+(x::Number, y::Number) = +(promote(x, y)...)`), and a *fresh* sjulia
# process does too. But sjulia's runtime dispatcher can loosely match a Float
# operand to the `::Integer` methods above once the specialization cache has
# been populated by a preceding mixed-type sweep (the #5966 / #7334
# loose-abstract-match class): `3//4 + Float32(2.5)` then wrongly selects
# `+(x::Rational, y::Integer)`, feeds a Float numerator into the `Rational`
# constructor, and aborts with `Inconsistent type inference for T`.
#
# Defining the mixed Rational/AbstractFloat methods explicitly gives the Float
# case a genuine, correct, more-specific method so the dispatcher never needs
# to loose-match a Float to `::Integer`. Each just promotes both operands to a
# common floating type (mirroring the upstream promotion result) and operates,
# so the value and type match upstream exactly.
function Base.:+(x::Rational, y::AbstractFloat)
    px, py = promote(x, y)
    return px + py
end
function Base.:+(x::AbstractFloat, y::Rational)
    px, py = promote(x, y)
    return px + py
end
function Base.:-(x::Rational, y::AbstractFloat)
    px, py = promote(x, y)
    return px - py
end
function Base.:-(x::AbstractFloat, y::Rational)
    px, py = promote(x, y)
    return px - py
end
function Base.:*(x::Rational, y::AbstractFloat)
    px, py = promote(x, y)
    return px * py
end
function Base.:*(x::AbstractFloat, y::Rational)
    px, py = promote(x, y)
    return px * py
end
function Base.:/(x::Rational, y::AbstractFloat)
    px, py = promote(x, y)
    return px / py
end
function Base.:/(x::AbstractFloat, y::Rational)
    px, py = promote(x, y)
    return px / py
end

# =============================================================================
# Rational non-Int64 arithmetic specializations (Issue #2497)
# =============================================================================
# Generic Rational methods are compiled with I64 field type assumptions
# (from the Rational{Int64} struct definition). Non-Int64 fields must use
# explicit specializations to preserve their concrete element type.

function Base.:+(x::Rational{Int32}, y::Rational{Int32})
    num = x.num * y.den + y.num * x.den
    den = x.den * y.den
    return Rational{Int32}(Int32(num), Int32(den))
end

function Base.:-(x::Rational{Int32}, y::Rational{Int32})
    num = x.num * y.den - y.num * x.den
    den = x.den * y.den
    return Rational{Int32}(Int32(num), Int32(den))
end

function Base.:*(x::Rational{Int32}, y::Rational{Int32})
    num = x.num * y.num
    den = x.den * y.den
    return Rational{Int32}(Int32(num), Int32(den))
end

function Base.:/(x::Rational{Int32}, y::Rational{Int32})
    num = x.num * y.den
    den = x.den * y.num
    return Rational{Int32}(Int32(num), Int32(den))
end

function div(x::Rational{Int32}, y::Rational{Int32})
    return div(Int32(x.num * y.den), Int32(x.den * y.num))
end

function fld(x::Rational{Int32}, y::Rational{Int32})
    return Int32(fld(Int32(x.num * y.den), Int32(x.den * y.num)))
end

function cld(x::Rational{Int32}, y::Rational{Int32})
    return Int32(cld(Int32(x.num * y.den), Int32(x.den * y.num)))
end

function rem(x::Rational{Int32}, y::Rational{Int32})
    return x - div(x, y) * y
end

function mod(x::Rational{Int32}, y::Rational{Int32})
    return x - fld(x, y) * y
end

function Base.:+(x::Rational{Int16}, y::Rational{Int16})
    num = x.num * y.den + y.num * x.den
    den = x.den * y.den
    return Rational{Int16}(Int16(num), Int16(den))
end

function Base.:-(x::Rational{Int16}, y::Rational{Int16})
    num = x.num * y.den - y.num * x.den
    den = x.den * y.den
    return Rational{Int16}(Int16(num), Int16(den))
end

function Base.:*(x::Rational{Int16}, y::Rational{Int16})
    num = x.num * y.num
    den = x.den * y.den
    return Rational{Int16}(Int16(num), Int16(den))
end

function Base.:/(x::Rational{Int16}, y::Rational{Int16})
    num = x.num * y.den
    den = x.den * y.num
    return Rational{Int16}(Int16(num), Int16(den))
end

function div(x::Rational{Int16}, y::Rational{Int16})
    return div(Int16(x.num * y.den), Int16(x.den * y.num))
end

function fld(x::Rational{Int16}, y::Rational{Int16})
    return Int16(fld(Int16(x.num * y.den), Int16(x.den * y.num)))
end

function cld(x::Rational{Int16}, y::Rational{Int16})
    return Int16(cld(Int16(x.num * y.den), Int16(x.den * y.num)))
end

function rem(x::Rational{Int16}, y::Rational{Int16})
    return x - div(x, y) * y
end

function mod(x::Rational{Int16}, y::Rational{Int16})
    return x - fld(x, y) * y
end

function Base.:+(x::Rational{Int8}, y::Rational{Int8})
    num = x.num * y.den + y.num * x.den
    den = x.den * y.den
    return Rational{Int8}(Int8(num), Int8(den))
end

function Base.:-(x::Rational{Int8}, y::Rational{Int8})
    num = x.num * y.den - y.num * x.den
    den = x.den * y.den
    return Rational{Int8}(Int8(num), Int8(den))
end

function Base.:*(x::Rational{Int8}, y::Rational{Int8})
    num = x.num * y.num
    den = x.den * y.den
    return Rational{Int8}(Int8(num), Int8(den))
end

function Base.:/(x::Rational{Int8}, y::Rational{Int8})
    num = x.num * y.den
    den = x.den * y.num
    return Rational{Int8}(Int8(num), Int8(den))
end

function div(x::Rational{Int8}, y::Rational{Int8})
    return div(Int8(x.num * y.den), Int8(x.den * y.num))
end

function fld(x::Rational{Int8}, y::Rational{Int8})
    return Int8(fld(Int8(x.num * y.den), Int8(x.den * y.num)))
end

function cld(x::Rational{Int8}, y::Rational{Int8})
    return Int8(cld(Int8(x.num * y.den), Int8(x.den * y.num)))
end

function rem(x::Rational{Int8}, y::Rational{Int8})
    return x - div(x, y) * y
end

function mod(x::Rational{Int8}, y::Rational{Int8})
    return x - fld(x, y) * y
end

function Base.:+(x::Rational{BigInt}, y::Rational{BigInt})
    num = x.num * y.den + y.num * x.den
    den = x.den * y.den
    return Rational(num, den)
end

function Base.:-(x::Rational{BigInt}, y::Rational{BigInt})
    num = x.num * y.den - y.num * x.den
    den = x.den * y.den
    return Rational(num, den)
end

function Base.:*(x::Rational{BigInt}, y::Rational{BigInt})
    num = x.num * y.num
    den = x.den * y.den
    return Rational(num, den)
end

function Base.:/(x::Rational{BigInt}, y::Rational{BigInt})
    num = x.num * y.den
    den = x.den * y.num
    return Rational(num, den)
end

# =============================================================================
# Comparison operators (Julia standard)
# =============================================================================

function Base.:(==)(x::Rational, y::Rational)
    return x.num == y.num && x.den == y.den
end

function Base.:<(x::Rational, y::Rational)
    return x.num * y.den < y.num * x.den
end

function Base.:<=(x::Rational, y::Rational)
    return x.num * y.den <= y.num * x.den
end

function Base.:>(x::Rational, y::Rational)
    return x.num * y.den > y.num * x.den
end

function Base.:>=(x::Rational, y::Rational)
    return x.num * y.den >= y.num * x.den
end

function Base.:(==)(x::Rational, y::Integer)
    return x.den == 1 && x.num == y
end

function Base.:(==)(x::Integer, y::Rational)
    return y == x
end

# Mixed Rational/Real `!=` (Issue #5975). Upstream relies on the generic
# `!=(x, y) = !(x == y)` (operators.jl); in this VM that fallback does not reach
# the `Rational × Integer` / `Rational × AbstractFloat` pairs, so they raised a
# MethodError even though the matching `==` works. Mirror the `==` methods with
# explicit `!=` that delegate to `!(x == y)` — recursion-safe (the `==` above
# terminates). Signatures are narrow (`Integer` / `AbstractFloat`, disjoint from
# `Complex`), so they do not introduce ambiguity with the `Complex` mixed `!=`,
# and `Rational × Rational` / `Rational × Complex` (which already work) are left
# untouched.
function Base.:(!=)(x::Rational, y::Integer)
    return !(x == y)
end

function Base.:(!=)(x::Integer, y::Rational)
    return !(x == y)
end

function Base.:(!=)(x::Rational, y::AbstractFloat)
    return !(x == y)
end

function Base.:(!=)(x::AbstractFloat, y::Rational)
    return !(x == y)
end

# =============================================================================
# Rational vs hardware-float comparisons (Issue #9340)
# =============================================================================
# Upstream base/rational.jl compares a Rational with an AbstractFloat *exactly*
# (infinite precision): `==` requires the rational's denominator to be a power
# of two and the float to reproduce the numerator exactly; `<`/`<=` cross-
# multiply the exact integer ratio of the float against the rational. The
# bundled base/rational.jl previously lacked these mixed methods, so the numeric
# operator promote-fallback handled them by rounding *both* operands to Float64
# first — making e.g. `1//3 == 0.3333333333333333` wrongly return `true`.
#
# Mirror upstream's exact semantics. Rather than depend on `decompose`/
# `ndigits0z` (not available here), decompose the finite float into an exact
# power-of-two ratio via `frexp` (x == m * 2^(e-53), m an exact integer) and
# compare in `BigInt` to avoid overflow. These signatures are strictly more
# specific than the generic `==`/`<`/`<=` promote fallback, so they win.
#
# Scope: all AbstractFloats. `Float16`/`Float32`/`Float64` widen to Float64
# losslessly, so the power-of-two ratio is exact; `BigFloat` (Issue #9424)
# decomposes at its own precision via `frexp` + an exact mantissa shift, so
# e.g. `1//3 == BigFloat(1)/BigFloat(3)` is `false` at any precision, exactly
# as upstream.

# Exact rational (num::BigInt, den::BigInt), den = 2^k >= 1, with x == num/den.
# Precondition: `x` is a finite hardware float. Sign is carried in `num`;
# `den` is positive.
function _rational_float_ratio(x::Union{Float16,Float32,Float64})
    f, e = frexp(Float64(x))
    m = round(Int64, ldexp(f, 53)) # |m| < 2^53, exact integer
    p = e - 53
    if p >= 0
        return (big(m) * big(2)^p, big(1))
    else
        return (big(m), big(2)^(-p))
    end
end

# BigFloat: decompose at the value's own precision (Issue #9424). `frexp`
# gives x == f·2^e with f ∈ [0.5, 1); scaling f by 2^prec is an exact
# astro_float exponent shift (`_bigfloat_scale2`, no rounding), producing an
# integer-valued BigFloat that the exact `BigInt` conversion reads verbatim.
function _rational_float_ratio(x::BigFloat)
    f, e = frexp(x)
    prec = precision(x)
    m = BigInt(_bigfloat_scale2(f, prec)) # |m| <= 2^prec, exact integer
    p = e - prec
    if p >= 0
        return (m * big(2)^p, big(1))
    else
        return (m, big(2)^(-p))
    end
end

function Base.:(==)(x::AbstractFloat, q::Rational)
    isnan(x) && return false
    isfinite(x) || return x == q.num / q.den
    xn, xd = _rational_float_ratio(x)
    return xn * big(q.den) == big(q.num) * xd
end

function Base.:(==)(q::Rational, x::AbstractFloat)
    return x == q
end

function Base.:<(x::AbstractFloat, q::Rational)
    isnan(x) && return false
    isfinite(x) || return x < q.num / q.den
    xn, xd = _rational_float_ratio(x)
    return xn * big(q.den) < big(q.num) * xd
end

function Base.:<(q::Rational, x::AbstractFloat)
    isnan(x) && return false
    isfinite(x) || return q.num / q.den < x
    xn, xd = _rational_float_ratio(x)
    return big(q.num) * xd < xn * big(q.den)
end

function Base.:<=(x::AbstractFloat, q::Rational)
    isnan(x) && return false
    isfinite(x) || return x <= q.num / q.den
    xn, xd = _rational_float_ratio(x)
    return xn * big(q.den) <= big(q.num) * xd
end

function Base.:<=(q::Rational, x::AbstractFloat)
    isnan(x) && return false
    isfinite(x) || return q.num / q.den <= x
    xn, xd = _rational_float_ratio(x)
    return big(q.num) * xd <= xn * big(q.den)
end

function Base.:>(x::AbstractFloat, q::Rational)
    return q < x
end

function Base.:>(q::Rational, x::AbstractFloat)
    return x < q
end

function Base.:>=(x::AbstractFloat, q::Rational)
    return q <= x
end

function Base.:>=(q::Rational, x::AbstractFloat)
    return x <= q
end

function Base.:<(x::Rational, y::Integer)
    return x.num < x.den * y
end

function Base.:<(x::Integer, y::Rational)
    return x * y.den < y.num
end

function Base.:<=(x::Rational, y::Integer)
    return x.num <= x.den * y
end

function Base.:<=(x::Integer, y::Rational)
    return x * y.den <= y.num
end

function Base.:>(x::Rational, y::Integer)
    return x.num > x.den * y
end

function Base.:>(x::Integer, y::Rational)
    return x * y.den > y.num
end

function Base.:>=(x::Rational, y::Integer)
    return x.num >= x.den * y
end

function Base.:>=(x::Integer, y::Rational)
    return x * y.den >= y.num
end

# Rational{BigInt} comparison specializations (Issue #2497)
function Base.:(==)(x::Rational{BigInt}, y::Rational{BigInt})
    return x.num == y.num && x.den == y.den
end

function Base.:<(x::Rational{BigInt}, y::Rational{BigInt})
    return x.num * y.den < y.num * x.den
end

function Base.:<=(x::Rational{BigInt}, y::Rational{BigInt})
    return x.num * y.den <= y.num * x.den
end

function Base.:>(x::Rational{BigInt}, y::Rational{BigInt})
    return x.num * y.den > y.num * x.den
end

function Base.:>=(x::Rational{BigInt}, y::Rational{BigInt})
    return x.num * y.den >= y.num * x.den
end

# Cross-type Rational{BigInt} comparison specializations (Issue #2511)
# When comparing Rational{BigInt} with Rational{IntN}, convert to Rational{BigInt} first
# to avoid EqInt intrinsic mismatch on BigInt fields.
function Base.:(==)(x::Rational{BigInt}, y::Rational{Int64})
    return x == Rational{BigInt}(big(y.num), big(y.den))
end
function Base.:(==)(x::Rational{Int64}, y::Rational{BigInt})
    return Rational{BigInt}(big(x.num), big(x.den)) == y
end
function Base.:(==)(x::Rational{BigInt}, y::Rational{Int32})
    return x == Rational{BigInt}(big(y.num), big(y.den))
end
function Base.:(==)(x::Rational{Int32}, y::Rational{BigInt})
    return Rational{BigInt}(big(x.num), big(x.den)) == y
end
function Base.:(==)(x::Rational{BigInt}, y::Rational{Int16})
    return x == Rational{BigInt}(big(y.num), big(y.den))
end
function Base.:(==)(x::Rational{Int16}, y::Rational{BigInt})
    return Rational{BigInt}(big(x.num), big(x.den)) == y
end
function Base.:(==)(x::Rational{BigInt}, y::Rational{Int8})
    return x == Rational{BigInt}(big(y.num), big(y.den))
end
function Base.:(==)(x::Rational{Int8}, y::Rational{BigInt})
    return Rational{BigInt}(big(x.num), big(x.den)) == y
end

# Cross-type Rational{BigInt} ordering specializations (Issue #2511)
function Base.:<(x::Rational{BigInt}, y::Rational{Int64})
    return x < Rational{BigInt}(big(y.num), big(y.den))
end
function Base.:<(x::Rational{Int64}, y::Rational{BigInt})
    return Rational{BigInt}(big(x.num), big(x.den)) < y
end
function Base.:<=(x::Rational{BigInt}, y::Rational{Int64})
    return x <= Rational{BigInt}(big(y.num), big(y.den))
end
function Base.:<=(x::Rational{Int64}, y::Rational{BigInt})
    return Rational{BigInt}(big(x.num), big(x.den)) <= y
end
function Base.:>(x::Rational{BigInt}, y::Rational{Int64})
    return x > Rational{BigInt}(big(y.num), big(y.den))
end
function Base.:>(x::Rational{Int64}, y::Rational{BigInt})
    return Rational{BigInt}(big(x.num), big(x.den)) > y
end
function Base.:>=(x::Rational{BigInt}, y::Rational{Int64})
    return x >= Rational{BigInt}(big(y.num), big(y.den))
end
function Base.:>=(x::Rational{Int64}, y::Rational{BigInt})
    return Rational{BigInt}(big(x.num), big(x.den)) >= y
end

# =============================================================================
# Math functions (Julia standard - using dispatch)
# =============================================================================

function abs(x::Rational)
    if x.num < 0
        return Rational(zero(x.num) - x.num, x.den)
    end
    return x
end

function sign(x::Rational)
    if x.num > 0
        return 1
    elseif x.num < 0
        return -1
    else
        return 0
    end
end

# floor/ceil/trunc/round on a Rational return a Rational (matching upstream
# Julia), not a Float64 (Issue #6775). Upstream builds these on
# `round(::Type{T}, x::Rational, r::RoundingMode) = convert(T, div(num, den, r))`;
# here we route through sjulia's integer `div(x, y, r::RoundingMode)` (Issue
# #5691) so the quotient is computed with exact integer division:
#   floor  -> RoundDown   (fld)
#   ceil   -> RoundUp     (cld)
#   trunc  -> RoundToZero (div)
#   round  -> RoundNearest (half-to-even)
# The bare forms wrap the integer quotient back in a Rational (preserving the
# element type via `Rational(::Integer)`); the typed `f(::Type{T}, x::Rational)`
# forms return the integer type `T`.
# `div(x.num, x.den, r)` may widen narrow integers (Int32/Int16/Int8) to Float64
# through the generic `fld`/`cld` fallback, so coerce the quotient back to the
# element type before wrapping in a Rational. This preserves `Rational{Int32}`
# etc. (upstream does `convert(T, div(numerator(x), denominator(x), r))`).
function floor(x::Rational)
    return Rational(typeof(x.num)(div(x.num, x.den, RoundDown)))
end

function ceil(x::Rational)
    return Rational(typeof(x.num)(div(x.num, x.den, RoundUp)))
end

function trunc(x::Rational)
    return Rational(typeof(x.num)(div(x.num, x.den, RoundToZero)))
end

function round(x::Rational)
    return Rational(typeof(x.num)(div(x.num, x.den, RoundNearest)))
end

# Typed forms: floor(Int, 7//2) === 3, etc. (return the integer type T).
function floor(::Type{T}, x::Rational) where {T<:Integer}
    return T(div(x.num, x.den, RoundDown))
end

function ceil(::Type{T}, x::Rational) where {T<:Integer}
    return T(div(x.num, x.den, RoundUp))
end

function trunc(::Type{T}, x::Rational) where {T<:Integer}
    return T(div(x.num, x.den, RoundToZero))
end

function round(::Type{T}, x::Rational) where {T<:Integer}
    return T(div(x.num, x.den, RoundNearest))
end

# =============================================================================
# Power operator (Julia standard)
# =============================================================================

function Base.:^(x::Rational{T}, n::Int64) where {T<:Integer}
    if n == 0
        return Rational{T}(one(x.num), one(x.den))
    end
    if n < 0
        x = inv(x)
        n = -n
    end
    return Rational{T}(x.num ^ n, x.den ^ n)
end

function Base.:^(x::Rational, n::Int64)
    if n == 0
        # Use x's type info: construct identity element without explicit T
        return Rational(one(x.num), one(x.den))
    end
    if n < 0
        x = inv(x)
        n = -n
    end
    # Start with identity: num=1, den=1 of same type as x
    result = Rational(one(x.num), one(x.den))
    for i in 1:n
        result = result * x
    end
    return result
end

# Float raised to a Rational exponent. Upstream (base/rational.jl):
#   ^(x::T, y::Rational) where {T<:AbstractFloat} = x^convert(T,y)
# converts the exponent to the base's float type and dispatches to the
# same-type float `^`, which raises a DomainError for a negative base with a
# non-integer exponent (e.g. `(-8.0)^(1//3)`) instead of returning NaN
# (Issue #9344).
Base.:^(x::T, y::Rational) where {T<:AbstractFloat} = x^convert(T, y)

# =============================================================================
# GCD/LCM (Julia standard - using dispatch)
# =============================================================================

function gcd(x::Rational, y::Rational)
    num = gcd(x.num, y.num)
    den = div(abs(x.den * y.den), gcd(x.den, y.den))
    return Rational(num, den)
end

function lcm(x::Rational, y::Rational)
    num = div(abs(x.num * y.num), gcd(x.num, y.num))
    den = gcd(x.den, y.den)
    return Rational(num, den)
end

# =============================================================================
# rationalize - Convert floating point to rational approximation
# =============================================================================
# Based on Julia's base/rational.jl
# Approximate floating point number as a Rational with given tolerance

# rationalize(x::Float64; tol::Real = eps(x)) - default to Int64
function rationalize(x::Float64; tol::Real = eps(x))
    return rationalize(Int64, x, tol)
end

# rationalize(::Type{Int64}, x::Float64; tol::Real = eps(x)) - type-specified
function rationalize(::Type{Int64}, x::Float64; tol::Real = eps(x))
    return rationalize(Int64, x, tol)
end

# Core rationalize implementation using Stern-Brocot tree / mediant method
# Simplified version that handles common decimal values well
function rationalize(::Type{Int64}, x::Float64, tol::Real)
    if tol < 0
        error("negative tolerance")
    end

    # Handle special cases
    if isnan(x)
        return Rational{Int64}(0, 0)  # NaN representation
    end
    if isinf(x)
        if x < 0
            return Rational{Int64}(-1, 0)  # -Inf
        else
            return Rational{Int64}(1, 0)    # +Inf
        end
    end

    # Handle zero
    if x == 0.0
        return Rational{Int64}(0, 1)
    end

    # Handle negative numbers
    sign_x = x < 0 ? -1 : 1
    x_abs = abs(x)

    # Simple approach: multiply by increasing powers of 10 until we get close to an integer
    # Then reduce the fraction
    max_denom = 10000000  # Limit denominator size

    # Try denominators 1, 2, 3, ..., up to max_denom
    # Find the one that gives the best approximation within tolerance
    best_num = Int64(round(x_abs))
    best_den = Int64(1)
    best_err = abs(x_abs - Float64(best_num))

    # First check small denominators explicitly for common fractions
    for den in 1:1000
        num = Int64(round(x_abs * den))
        err = abs(x_abs - Float64(num) / Float64(den))
        if err < best_err
            best_err = err
            best_num = num
            best_den = den
        end
        # If we found an exact match (within tolerance), stop
        if err <= tol
            break
        end
    end

    # Reduce the fraction using gcd
    g = gcd(best_num, best_den)
    result_num = sign_x * div(best_num, g)
    result_den = div(best_den, g)

    return Rational{Int64}(result_num, result_den)
end

# rationalize(x::Rational) - already rational, return as-is
function rationalize(x::Rational)
    return x
end

# rationalize(x::Int64) - integer to rational
function rationalize(x::Int64)
    return Rational{Int64}(x, 1)
end

# rationalize(::Type{Int64}, x::Rational; tol::Real = 0) - convert rational type
function rationalize(::Type{Int64}, x::Rational; tol::Real = 0)
    # Already rational, just ensure type is Int64
    return Rational{Int64}(Int64(x.num), Int64(x.den))
end

# rationalize(::Type{Int64}, x::Int64) - integer to rational with type
function rationalize(::Type{Int64}, x::Int64)
    return Rational{Int64}(x, 1)
end

# =============================================================================
# div, fld, cld for Rational (Julia standard)
# =============================================================================
# Based on Julia's base/rational.jl:551-566
# These reduce to integer division of cross-multiplied values,
# avoiding floating-point entirely.

# div (truncated): div(a//b, c//d) = div(a*d, b*c)
function div(x::Rational, y::Rational)
    return div(x.num * y.den, x.den * y.num)
end

# Mixed Rational/Integer div mirrors upstream's `div(x, y, RoundToZero)` path
# (julia/base/rational.jl:503-509): the cross-multiplied operands go through the
# 3-arg integer `div(a, b, RoundToZero)`, which for a mixed Signed/Unsigned pair
# promotes to the common (unsigned) type *before* dividing — unlike the 2-arg
# `div`, whose #9337 signedness rule keeps the signed type. Promote here so the
# quotient's element type matches upstream (e.g. `div(1//1, UInt64(3))::UInt64`,
# not `::Int64`). Issue #9440.
function div(x::Rational, y::Integer)
    a, b = promote(x.num, x.den * y)
    return div(a, b)
end

function div(x::Integer, y::Rational)
    a, b = promote(x * y.den, y.num)
    return div(a, b)
end

# fld (floored): fld(a//b, c//d) = fld(a*d, b*c)
function fld(x::Rational, y::Rational)
    return fld(x.num * y.den, x.den * y.num)
end

function fld(x::Rational, y::Integer)
    return fld(x.num, x.den * y)
end

function fld(x::Integer, y::Rational)
    return fld(x * y.den, y.num)
end

# cld (ceiled): cld(a//b, c//d) = cld(a*d, b*c)
function cld(x::Rational, y::Rational)
    return cld(x.num * y.den, x.den * y.num)
end

function cld(x::Rational, y::Integer)
    return cld(x.num, x.den * y)
end

function cld(x::Integer, y::Rational)
    return cld(x * y.den, y.num)
end

# =============================================================================
# rem and mod for Rational (Julia standard)
# =============================================================================
# Based on Julia's base/rational.jl:408-436
# rem(x, y) = x - div(x, y) * y (truncated remainder)
# mod(x, y) = x - fld(x, y) * y (floored remainder)

function rem(x::Rational, y::Rational)
    return x - div(x, y) * y
end

function mod(x::Rational, y::Rational)
    return x - fld(x, y) * y
end

# Mixed: Rational / Integer — mirror upstream's direct rem/mod formulas
# (julia/base/rational.jl:383-404) rather than `x - div(x,y)*y`. Upstream applies
# the integer `rem`/`mod` to the cross-multiplied numerator and wraps the result
# with the ORIGINAL denominator, so the element type follows the integer
# `rem`/`mod` signedness rule: `rem(1//1, UInt64(3))` yields `Rational{Int64}`
# (rem(Int, UInt) is signed) while `mod(1//1, UInt64(3))` yields
# `Rational{UInt64}` (mod(Int, UInt) is unsigned). The `x - div(x,y)*y` form
# forced the whole result to the unsigned promotion. Issue #9440.
# The cross-multiplication uses `checked_mul` like upstream, so a boundary
# operand (e.g. `rem(3//4, typemax(UInt128))`) raises upstream's catchable
# OverflowError instead of silently wrapping (Issues #9416 / #9422).
function rem(x::Rational, y::Integer)
    return Rational(rem(x.num, checked_mul(x.den, y)), x.den)
end

function mod(x::Rational, y::Integer)
    return Rational(mod(x.num, checked_mul(x.den, y)), x.den)
end

# Mixed: Integer / Rational (julia/base/rational.jl:397-404)
function rem(y::Integer, x::Rational)
    return Rational(rem(checked_mul(x.den, y), x.num), x.den)
end

function mod(y::Integer, x::Rational)
    return Rational(mod(checked_mul(x.den, y), x.num), x.den)
end

# Mixed: Rational × Real (AbstractFloat, BigFloat, …). Upstream reaches these
# through the generic `rem(x::Real, y::Real) = rem(promote(x,y)...)` /
# `mod(x::Real, y::Real)` fallbacks in julia/base/promotion.jl. sjulia's
# untyped generic rem/mod (base/math.jl) instead fall through to the `%`
# builtin, which has no Float×Rational path → MethodError (Issue #9416).
# Scope the promote fallback to Rational-involved pairs so unrelated Real
# pairs cannot enter the promote-fallback recursion trap (Issue #5966); the
# more-specific Rational×Rational and Rational×Integer methods above still win
# for those pairs, and `promote` raises `sametype_error` if it cannot widen.
function rem(x::Rational, y::Real)
    px, py = promote(x, y)
    return rem(px, py)
end

function rem(x::Real, y::Rational)
    px, py = promote(x, y)
    return rem(px, py)
end

function mod(x::Rational, y::Real)
    px, py = promote(x, y)
    return mod(px, py)
end

function mod(x::Real, y::Rational)
    px, py = promote(x, y)
    return mod(px, py)
end
