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

eltype(x::Bool) = Bool
eltype(x::Int8) = Int8
eltype(x::Int16) = Int16
eltype(x::Int32) = Int32
eltype(x::Int64) = Int64
eltype(x::Int128) = Int128
eltype(x::UInt8) = UInt8
eltype(x::UInt16) = UInt16
eltype(x::UInt32) = UInt32
eltype(x::UInt64) = UInt64
eltype(x::UInt128) = UInt128
eltype(x::Float16) = Float16
eltype(x::Float32) = Float32
eltype(x::Float64) = Float64
eltype(x::BigInt) = BigInt
eltype(x::BigFloat) = BigFloat

# zero(x): return zero with the same type as x
# Based on Julia's base/number.jl
function zero(x::Int64)
    return Int64(0)
end

function zero(x::Float64)
    return 0.0
end

# Float32/Float16: type-preserving zero. Upstream `zero(x::Number)=oftype(x,0)`
# (julia/base/number.jl:363) preserves the concrete type. Explicit methods
# added so DIRECT and untyped calls keep the concrete float type rather than
# widening to Float64 (Issue #5167, follow-up to #5076).
function zero(x::Float32)
    return Float32(0)
end

function zero(x::Float16)
    return Float16(0)
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

# Unsigned integers: type-preserving zero (Issue #8220). Without these methods
# `zero(0x05)` fell through to a generic that returned `Int64`, losing the
# unsigned type (upstream returns the same UInt type as the argument).
function zero(x::UInt8)
    return UInt8(0)
end

function zero(x::UInt16)
    return UInt16(0)
end

function zero(x::UInt32)
    return UInt32(0)
end

function zero(x::UInt64)
    return UInt64(0)
end

function zero(x::UInt128)
    return UInt128(0)
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

# Float32/Float16: type-preserving one. Upstream `one(x::T) where {T<:Number}
# = one(T)` (julia/base/number.jl:406) preserves the concrete type. Without
# these methods `one(2.0f0)` / `one(Float16(1))` errored NoMethodFound even
# for DIRECT and untyped calls (Issue #5167, follow-up to #5076).
function one(x::Float32)
    return Float32(1)
end

function one(x::Float16)
    return Float16(1)
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

# Unsigned integers: type-preserving one (Issue #8220). Without these methods
# `one(0x05)` fell through to a generic that returned `Int64` (or errored
# NoMethodFound in some static contexts), losing the unsigned type; upstream
# returns the same UInt type as the argument.
function one(x::UInt8)
    return UInt8(1)
end

function one(x::UInt16)
    return UInt16(1)
end

function one(x::UInt32)
    return UInt32(1)
end

function one(x::UInt64)
    return UInt64(1)
end

function one(x::UInt128)
    return UInt128(1)
end

function one(x::Bool)
    return true
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
# INTENTIONAL_NOOP (Issue #4703): upstream `real(x::Real) = x`
# (julia/base/complex.jl:88) is identity for reals, so a `return x` body
# is correct. (complex.jl additionally provides the typed `real(x::Real)`
# method.)
function real(x)
    return x
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
function conj(x)
    return x
end

# isreal: check if value is real (imaginary part is zero)
# Note: For non-complex types, this always returns true
# For complex numbers, use imag(x) == 0 directly
# This simplified version only handles real numbers
# INTENTIONAL_NOOP (Issue #4703): upstream `isreal(x::Real) = true`
# (julia/base/complex.jl:147) returns the constant `true` for reals, so a
# `return true` body is the correct semantics for the real fallback.
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
function float(::Type{Float64})
    return Float64
end

function float(::Type{Float32})
    return Float32
end

function float(::Type{Float16})
    return Float16
end

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
function float(::Type{Int64})
    return Float64
end

function float(::Type{Int32})
    return Float64
end

function float(::Type{Int16})
    return Float64
end

function float(::Type{Int8})
    return Float64
end

function float(::Type{UInt64})
    return Float64
end

function float(::Type{UInt32})
    return Float64
end

function float(::Type{UInt16})
    return Float64
end

function float(::Type{UInt8})
    return Float64
end

function float(::Type{Bool})
    return Float64
end

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
# signed / unsigned: bit-pattern reinterpretation between same-width
# integer types (Issue #3727).
#
# Based on Julia's base/int.jl. signed(::Signed) and unsigned(::Unsigned)
# are identities; the cross-sign methods reinterpret the bit pattern.
# `BuiltinId::Signed` / `BuiltinId::Unsigned` remain as a fallback for types
# not yet covered here, but in normal use these Pure Julia methods win.
# =============================================================================

# signed: identity on already-signed integers
function signed(x::Int8)
    return x
end

function signed(x::Int16)
    return x
end

function signed(x::Int32)
    return x
end

function signed(x::Int64)
    return x
end

function signed(x::Int128)
    return x
end

# signed: bit-reinterpret unsigned integers as signed
function signed(x::UInt8)
    return reinterpret(Int8, x)
end

function signed(x::UInt16)
    return reinterpret(Int16, x)
end

function signed(x::UInt32)
    return reinterpret(Int32, x)
end

function signed(x::UInt64)
    return reinterpret(Int64, x)
end

function signed(x::UInt128)
    return reinterpret(Int128, x)
end

# signed: Bool -> Int64 (Julia widens Bool to Int)
function signed(x::Bool)
    return Int64(x)
end

# unsigned: identity on already-unsigned integers
function unsigned(x::UInt8)
    return x
end

function unsigned(x::UInt16)
    return x
end

function unsigned(x::UInt32)
    return x
end

function unsigned(x::UInt64)
    return x
end

function unsigned(x::UInt128)
    return x
end

# unsigned: bit-reinterpret signed integers as unsigned
function unsigned(x::Int8)
    return reinterpret(UInt8, x)
end

function unsigned(x::Int16)
    return reinterpret(UInt16, x)
end

function unsigned(x::Int32)
    return reinterpret(UInt32, x)
end

function unsigned(x::Int64)
    return reinterpret(UInt64, x)
end

function unsigned(x::Int128)
    return reinterpret(UInt128, x)
end

# unsigned: Bool -> UInt64 (Julia widens Bool to UInt). Constructor
# UInt64(::Bool) is not supported by the VM, so widen via Int64 first
# and reinterpret.
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
