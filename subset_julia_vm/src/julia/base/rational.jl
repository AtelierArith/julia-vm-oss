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

# Rational number struct (parametric version)
# Julia's Rational is Rational{T<:Integer} <: Real
struct Rational{T<:Integer} <: Real
    num::T
    den::T
    # Inner constructor: raw, no normalization (unsafe_rational equivalent)
    function Rational{T}(num::T, den::T) where T
        return new{T}(num, den)
    end
end

# =============================================================================
# Raw terminal constructor (Issue #5132)
# =============================================================================
# Mirror of Base.unsafe_rational: an unexported, no-normalization terminal.
# It reaches the raw inner constructor via the `where T` (type-variable)
# constructor path, which the compiler routes through bare-name dispatch and so
# never intercepts as an explicit `Rational{IntN}(...)` call. The explicit-typed
# public constructors below delegate here for the final allocation, so they never
# recurse into themselves.
function unsafe_rational(::Type{T}, num::T, den::T) where {T<:Integer}
    return Rational{T}(num, den)
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

# =============================================================================
# Outer constructors with GCD normalization (concrete types)
# =============================================================================
# Each concrete type has its own constructor to avoid Issue #2384
# (div(x::T, y::T) in where T context dispatches to generic div returning Float64)

# Int64 outer constructor with normalization
function Rational(num::Int64, den::Int64)
    if den == Int64(0)
        return Rational{Int64}(num, Int64(0))
    end
    if den < Int64(0)
        num = Int64(0) - num
        den = Int64(0) - den
    end
    g = gcd(num, den)
    if g > Int64(1)
        num = div(num, g)
        den = div(den, g)
    end
    return Rational{Int64}(num, den)
end

# Int32 outer constructor with normalization
function Rational(num::Int32, den::Int32)
    if den == Int32(0)
        return Rational{Int32}(num, Int32(0))
    end
    if den < Int32(0)
        num = Int32(0) - num
        den = Int32(0) - den
    end
    g = gcd(num, den)
    if g > Int32(1)
        num = div(num, g)
        den = div(den, g)
    end
    return Rational{Int32}(num, den)
end

# Int16 outer constructor with normalization
function Rational(num::Int16, den::Int16)
    if den == Int16(0)
        return Rational{Int16}(num, Int16(0))
    end
    if den < Int16(0)
        num = Int16(0) - num
        den = Int16(0) - den
    end
    g = gcd(num, den)
    if g > Int16(1)
        num = div(num, g)
        den = div(den, g)
    end
    return Rational{Int16}(num, den)
end

# Int8 outer constructor with normalization
function Rational(num::Int8, den::Int8)
    if den == Int8(0)
        return Rational{Int8}(num, Int8(0))
    end
    if den < Int8(0)
        num = Int8(0) - num
        den = Int8(0) - den
    end
    g = gcd(num, den)
    if g > Int8(1)
        num = div(num, g)
        den = div(den, g)
    end
    return Rational{Int8}(num, den)
end

# BigInt outer constructor with normalization (Issue #2497)
function Rational(num::BigInt, den::BigInt)
    if den == big(0)
        return Rational{BigInt}(num, big(0))
    end
    if den < big(0)
        num = big(0) - num
        den = big(0) - den
    end
    g = gcd(num, den)
    if g > big(1)
        num = div(num, g)
        den = div(den, g)
    end
    return Rational{BigInt}(num, den)
end

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

# Mixed-type constructor: promote both args to a common type
function Rational(num::Integer, den::Integer)
    pn, pd = promote(num, den)
    return Rational(pn, pd)
end

# =============================================================================
# // operator: creates Rational from two integers
# =============================================================================
# Based on Julia's base/rational.jl:91: //(n::Integer, d::Integer) = Rational(n,d)
# Concrete type methods to ensure correct compile-time dispatch to the
# matching Rational outer constructor (avoids Issue #2384 style widening
# when abstract Integer param causes dispatch to Int64 method).
function //(n::Int64, d::Int64)
    return Rational(n, d)
end
function //(n::Int32, d::Int32)
    return Rational(n, d)
end
function //(n::Int16, d::Int16)
    return Rational(n, d)
end
function //(n::Int8, d::Int8)
    return Rational(n, d)
end
function //(n::BigInt, d::BigInt)
    return Rational(n, d)
end
# Mixed-type fallback: promote to common type
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
    return x.num == 0 && x.den != 0
end

function isone(x::Rational)
    return x.num == 1 && x.den == 1
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

function Base.:+(x::Rational, y::Integer)
    return Rational(x.num + x.den * y, x.den)
end

function Base.:+(y::Integer, x::Rational)
    return Rational(y * x.den + x.num, x.den)
end

function Base.:-(x::Rational, y::BigInt)
    return convert(Rational{BigInt}, x) - Rational{BigInt}(y, big(1))
end

function Base.:-(y::BigInt, x::Rational)
    return Rational{BigInt}(y, big(1)) - convert(Rational{BigInt}, x)
end

function Base.:-(x::Rational, y::Integer)
    return Rational(x.num - x.den * y, x.den)
end

function Base.:-(y::Integer, x::Rational)
    return Rational(y * x.den - x.num, x.den)
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

function div(x::Rational, y::Integer)
    return div(x.num, x.den * y)
end

function div(x::Integer, y::Rational)
    return div(x * y.den, y.num)
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

# Mixed: Rational / Integer
function rem(x::Rational, y::Integer)
    return x - div(x, y) * y
end

function mod(x::Rational, y::Integer)
    return x - fld(x, y) * y
end

# Mixed: Integer / Rational
function rem(y::Integer, x::Rational)
    return y - div(y, x) * x
end

function mod(y::Integer, x::Rational)
    return y - fld(y, x) * x
end

function rem(y::Int64, x::Rational{Int64})
    return y - div(y, x) * x
end

function mod(y::Int64, x::Rational{Int64})
    return y - fld(y, x) * x
end
