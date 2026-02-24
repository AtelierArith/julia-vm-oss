# =============================================================================
# Number - Number type predicates and operations
# =============================================================================
# Based on Julia's base/number.jl

# iszero: check if value is zero
function iszero(x)
    return x == 0
end

# iszero for BigInt: compare with BigInt(0)
function iszero(x::BigInt)
    return x == BigInt(0)
end

# isone: check if value is one
function isone(x)
    return x == 1
end

# isone for BigInt: compare with BigInt(1)
function isone(x::BigInt)
    return x == BigInt(1)
end

# zero(x): return zero with the same type as x
# Based on Julia's base/number.jl
function zero(x::Int64)
    return Int64(0)
end

function zero(x::Float64)
    return 0.0
end

function zero(x::BigInt)
    return BigInt(0)
end

function zero(x::Int32)
    return Int32(0)
end

function zero(x::Int16)
    return Int16(0)
end

function zero(x::Int8)
    return Int8(0)
end

function zero(x::Bool)
    return false
end

# one(x): return one with the same type as x
# Based on Julia's base/number.jl
function one(x::Int64)
    return Int64(1)
end

function one(x::Float64)
    return 1.0
end

function one(x::BigInt)
    return BigInt(1)
end

function one(x::Int32)
    return Int32(1)
end

function one(x::Int16)
    return Int16(1)
end

function one(x::Int8)
    return Int8(1)
end

function one(x::Bool)
    return true
end

# identity: return the input unchanged
function identity(x)
    return x
end

# oneunit: return one with the same type (simplified)
function oneunit(x)
    return 1
end

# signbit: check if the sign bit is set (negative)
# Based on Julia's base/number.jl:137
# This is the generic fallback for Real numbers
function signbit(x)
    return x < 0
end

# isnegative: check if value is negative (x < 0)
# Based on Julia's base/number.jl (added in Julia 1.12, PR #53677)
function isnegative(x)
    return x < zero(x)
end

# ispositive: check if value is positive (x > 0)
# Based on Julia's base/number.jl (added in Julia 1.12, PR #53677)
function ispositive(x)
    return x > zero(x)
end

# flipsign: flip sign of x if y is negative
# Based on Julia's base/number.jl:249
# Generic fallback for Real numbers
function flipsign(x, y)
    if signbit(y)
        return -x
    else
        return +x  # the + is for type-stability on Bool
    end
end

# abs: absolute value for real numbers
# Based on Julia's base/number.jl:208
# Generic fallback using signbit (Complex version is in complex.jl)
function abs(x)
    if signbit(x)
        return -x
    else
        return x
    end
end

# abs2: squared absolute value for real numbers
# Complex version is in complex.jl with abs2(z::Complex)
function abs2(x)
    return x * x
end

# real: fallback for non-complex types (returns the value itself)
# Complex version is in complex.jl with real(z::Complex)
function real(x)
    return x
end

# imag: fallback for non-complex types (imaginary part is zero)
# Complex version is in complex.jl with imag(z::Complex)
# Note: Returns 0.0 (Float64) for type consistency with imag(z::Complex)
function imag(x)
    return 0.0
end

# conj: fallback for non-complex types (conjugate is identity)
# Complex version is in complex.jl with conj(z::Complex)
function conj(x)
    return x
end

# isreal: check if value is real (imaginary part is zero)
# Note: For non-complex types, this always returns true
# For complex numbers, use imag(x) == 0 directly
# This simplified version only handles real numbers
function isreal(x)
    return true
end

# =============================================================================
# Type conversion: float
# =============================================================================
# Based on Julia's base/float.jl
# Convert a number to Float64

# float(x) - convert to floating point type
# Based on Julia's base/float.jl:375
# For AbstractFloat types: identity (preserves type)
# For Integer types: convert to Float64

# Identity for AbstractFloat types
function float(x::Float64)
    return x
end

function float(x::Float32)
    return x
end

function float(x::Float16)
    return x
end

# Integer types -> Float64
function float(x::Int64)
    return Float64(x)
end

function float(x::Int32)
    return Float64(x)
end

function float(x::Int16)
    return Float64(x)
end

function float(x::Int8)
    return Float64(x)
end

function float(x::Int128)
    return Float64(x)
end

function float(x::UInt8)
    return Float64(x)
end

function float(x::UInt16)
    return Float64(x)
end

function float(x::UInt32)
    return Float64(x)
end

function float(x::UInt64)
    return Float64(x)
end

function float(x::UInt128)
    return Float64(x)
end

# Bool -> Float64 (Issue #2722)
function float(x::Bool)
    return Float64(x)
end

# =============================================================================
# Number linear algebra methods
# =============================================================================
# Based on Julia's base/number.jl:268-299
# Note: Many Number iteration methods (size, length, first, last, iterate, etc.)
# have VM builtin implementations. Those definitions are omitted here to avoid
# dispatch conflicts with other types like SkipMissing iterators.

function inv(x::Number)
    return one(x) / x
end

# =============================================================================
# transpose and adjoint for scalars
# =============================================================================
# Based on Julia's base/number.jl:268-269
# transpose(x::Number) = x
# adjoint(x::Number) = conj(x)

# transpose for real scalars (identity)
function transpose(x::Real)
    return x
end

# adjoint for real scalars (identity, since conj(x) = x for reals)
function adjoint(x::Real)
    return x
end

# =============================================================================
# Number predicates
# =============================================================================
# Based on Julia's base/number.jl:20,78

# Note: isinteger for Integer types is handled by the generic fallback in floatfuncs.jl
# Adding isinteger(x::Integer) here can cause dispatch issues with tanpi

# isfinite for Int64 - integers are always finite
function isfinite(x::Int64)
    return true
end

# isfinite for Float64 - check if not Inf or NaN
function isfinite(x::Float64)
    return iszero(x - x)
end

# =============================================================================
# map for scalar numbers
# =============================================================================
# Based on Julia's base/number.jl:328
# map(f, x::Number, ys::Number...) = f(x, ys...)
#
# Note: Full variadic splat (f(x, ys...)) isn't supported yet, so we provide
# explicit overloads for common arities (1-4 arguments).
# We use concrete types (Int64, Float64) to avoid dispatch conflicts with map(f, Array).

# Single argument - Int64
function map(f, x::Int64)
    return f(x)
end

# Single argument - Float64
function map(f, x::Float64)
    return f(x)
end

# Two arguments - Int64, Int64
function map(f, x::Int64, y::Int64)
    return f(x, y)
end

# Two arguments - Float64, Float64
function map(f, x::Float64, y::Float64)
    return f(x, y)
end

# Two arguments - Int64, Float64
function map(f, x::Int64, y::Float64)
    return f(x, y)
end

# Two arguments - Float64, Int64
function map(f, x::Float64, y::Int64)
    return f(x, y)
end

# Three arguments - Int64
function map(f, x::Int64, y::Int64, z::Int64)
    return f(x, y, z)
end

# Three arguments - Float64
function map(f, x::Float64, y::Float64, z::Float64)
    return f(x, y, z)
end

# Four arguments - Int64
function map(f, x::Int64, y::Int64, z::Int64, w::Int64)
    return f(x, y, z, w)
end

# Four arguments - Float64
function map(f, x::Float64, y::Float64, z::Float64, w::Float64)
    return f(x, y, z, w)
end

# =============================================================================
# widemul: multiply with widening to avoid overflow
# =============================================================================
# Based on Julia's base/number.jl:321
# widemul(x, y) = widen(x) * widen(y)

"""
    widemul(x, y)

Multiply `x` and `y`, giving the result as a larger type to avoid overflow.

# Examples
```julia
widemul(Int32(1000000), Int32(1000000))  # returns Int64(1000000000000)
```
"""
function widemul(x, y)
    return widen(x) * widen(y)
end
