# =============================================================================
# Number - Number type predicates and operations
# =============================================================================
# Based on Julia's base/number.jl
# upstream: julia/base/number.jl @ 15346901f0039751c5488744f1f62de7d87510a8 (swept 2026-07-02)

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

# Upstream Julia treats scalar numbers as zero-dimensional collections whose
# element type is the scalar type. This feeds iterator collect paths such as
# Iterators.flatten((1, 2)).
function eltype(::Type{T}) where {T<:Number}
    return T
end

function eltype(x::Number)
    return typeof(x)
end

# Type-level ndims for numbers (Issue #5118). Upstream (julia/base/number.jl:91-92)
# defines `ndims(x::Number) = 0` and `ndims(::Type{<:Number}) = 0`, so both the
# value form (handled as a 0-dim scalar carrier by the VM) and the type form
# report a zero-dimensional collection. The VM cannot bind a covariant
# `::Type{<:Number}` parameter, so the type method dispatches on the concrete
# `::Type{T} where {T<:Number}` form the dispatcher resolves.
ndims(::Type{T}) where {T<:Number} = 0

# Upstream value forms delegate to the type form instead of enumerating every
# primitive numeric width. `zero(::Type{T})` is defined in promotion.jl and
# `one(::Type{T})` in complex.jl; dispatch resolves them after Base is loaded.
function zero(x::T) where {T<:Number}
    return zero(T)
end

function one(x::T) where {T<:Number}
    return one(T)
end

# identity: return the input unchanged
# INTENTIONAL_NOOP (Issue #4703): upstream `identity(@nospecialize x) = x`
# (julia/base/operators.jl:584) is itself a pass-through, so a `return x`
# body is the correct semantics, not an unfinished stub.
function identity(x)
    return x
end

# oneunit: return one of the same type as the argument (type-preserving).
# Based on Julia's base/number.jl:430-431:
#   oneunit(x::T) where {T} = T(one(x))
#   oneunit(::Type{T}) where {T} = T(one(T))
# Unlike `one` (a dimensionless multiplicative identity), `oneunit` is
# dimensionful: it returns a value of the same type as `x` (or of type
# `T`). e.g. `oneunit(3.0)` is `1.0::Float64`, `oneunit(Int8(5))` is
# `1::Int8`. (Issue #5039 replaced the prior untyped `oneunit(x)=1` stub
# that silently returned `1::Int64` for every input.)
function oneunit(x::T) where {T}
    return T(one(x))
end

function oneunit(::Type{T}) where {T}
    return T(one(T))
end

# signbit: check if the sign bit is set (negative)
# Based on Julia's base/number.jl:137
# This is the generic fallback for Real numbers. Keep the dispatch boundary
# explicit so `applicable(signbit, nonreal)` matches upstream (Issue #11797).
function signbit(x::Real)
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
# Based on Julia's base/number.jl:205.
# The fallback is restricted to Real just like upstream; the old untyped
# signature let String reach unary +/- and raised TypeError (Issue #11525).
function flipsign(x::Real, y::Real)
    if signbit(y)
        return -x
    else
        return +x  # the + is for type-stability on Bool
    end
end

# abs: absolute value for real numbers
# Based on Julia's base/number.jl:208
# Generic fallback using signbit (Complex version is in complex.jl)
function abs(x::Real)
    if signbit(x)
        return -x
    else
        return x
    end
end

# abs2: squared absolute value for real numbers
# Complex version is in complex.jl with abs2(z::Complex)
# Based on Julia's base/number.jl:189 (abs2(x::Real) = x*x).
# Was an untyped `function abs2(x)`, so it silently matched non-numeric
# args too: abs2("a") dispatched here and `"a" * "a"` string-concatenated
# to "aa" instead of raising a MethodError like upstream (Issue #10602).
function abs2(x::Real)
    return x * x
end

# real: fallback for non-complex types (returns the value itself)
# Complex version is in complex.jl with real(z::Complex)
# INTENTIONAL_NOOP (Issue #4703): upstream `real(x::Real) = x`
# (julia/base/complex.jl:88) is identity for reals, so a `return x` body
# is correct. (complex.jl additionally provides the typed `real(x::Real)`
# method.)
function real(x::Real)
    return x
end

function real(::Type{T}) where {T<:Real}
    return T
end

# imag: NO untyped fallback here on purpose (Issue #5039).
# Upstream Base defines only the typed `imag(x::Real) = zero(x)` and
# `imag(z::Complex) = z.im` (julia/base/complex.jl:87-89); there is no
# untyped `imag(x)` method. The previous untyped `imag(x) = 0.0` stub
# returned `0.0::Float64` for every input, losing the argument type (e.g.
# `imag(3)` gave `0.0::Float64` instead of `0::Int64`). It is removed so
# the type-preserving `imag(x::Real) = zero(x)` in complex.jl is selected.

# conj: fallback for non-complex types (conjugate is identity)
# Complex version is in complex.jl with conj(z::Complex)
# INTENTIONAL_NOOP (Issue #4703): upstream `conj(x::Real) = x`
# (julia/base/number.jl:273) is identity for reals, so a `return x` body
# is correct.
# The old untyped signature silently returned non-numeric inputs unchanged
# instead of rejecting them at dispatch (Issue #11522).
function conj(x::Real)
    return x
end

# isreal: check if value is real (imaginary part is zero)
# Note: For non-complex types, this always returns true
# For complex numbers, use imag(x) == 0 directly
# This simplified version only handles real numbers
# INTENTIONAL_NOOP (Issue #4703): upstream `isreal(x::Real) = true`
# (julia/base/complex.jl:147) returns the constant `true` for reals, so a
# `return true` body is the correct semantics for the real fallback.
# The old untyped signature silently returned true for non-numeric inputs
# instead of rejecting them at dispatch (Issue #11522).
function isreal(x::Real)
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

# Identity for AbstractFloat types; fixed-width integers widen to Float64,
# while BigInt follows upstream's arbitrary-precision BigFloat path.
function float(::Type{T}) where {T<:AbstractFloat}
    return T
end

function float(x::T) where {T<:AbstractFloat}
    return x
end

function float(::Type{T}) where {T<:Integer}
    return T === BigInt ? BigFloat : Float64
end

function float(x::Integer)
    return x isa BigInt ? BigFloat(x) : Float64(x)
end

function float(::Type{T}) where {T<:Number}
    return typeof(float(zero(T)))
end

# =============================================================================
# signed / unsigned: bit-pattern reinterpretation between same-width integer
# types (Issue #3727). Based on Julia's base/int.jl, but kept here because
# number.jl loads before int.jl in bundled Base.
# =============================================================================

function signed(::Type{Bool})
    return Int
end

function signed(::Type{T}) where {T<:Signed}
    return T
end

function signed(::Type{T}) where {T<:Unsigned}
    if T === UInt8
        return Int8
    elseif T === UInt16
        return Int16
    elseif T === UInt32
        return Int32
    elseif T === UInt64
        return Int64
    else
        return Int128
    end
end

function signed(x::T) where {T<:Signed}
    return x
end

function signed(x::T) where {T<:Unsigned}
    return reinterpret(signed(T), x)
end

function signed(x::Bool)
    return Int64(x)
end

function unsigned(::Type{Bool})
    return UInt
end

function unsigned(::Type{T}) where {T<:Unsigned}
    return T
end

function unsigned(::Type{T}) where {T<:Signed}
    if T === Int8
        return UInt8
    elseif T === Int16
        return UInt16
    elseif T === Int32
        return UInt32
    elseif T === Int64
        return UInt64
    else
        return UInt128
    end
end

function unsigned(x::T) where {T<:Unsigned}
    return x
end

function unsigned(x::T) where {T<:Signed}
    return reinterpret(unsigned(T), x)
end

function unsigned(x::Bool)
    return reinterpret(UInt64, Int64(x))
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
