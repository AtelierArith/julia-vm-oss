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

# Rational{BigInt} unary specializations (Issue #2497)
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

# =============================================================================
# Rational{BigInt} arithmetic specializations (Issue #2497)
# =============================================================================
# Generic Rational methods are compiled with I64 field type assumptions
# (from the Rational{Int64} struct definition). Rational{BigInt} fields
# contain BigInt values, so we need explicit specializations.

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

function floor(x::Rational)
    return floor(x.num / x.den)
end

function ceil(x::Rational)
    return ceil(x.num / x.den)
end

function round(x::Rational)
    return round(x.num / x.den)
end

# =============================================================================
# Power operator (Julia standard)
# =============================================================================

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

