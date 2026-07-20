# =============================================================================
# Irrationals - Irrational mathematical constants
# =============================================================================
# Based on Julia's base/irrationals.jl
#
# Provides the Irrational{sym} type for exact representation of irrational
# constants like pi, e, etc. Numeric constructors are handled by the VM so
# Float64(pi) and BigFloat(pi) can preserve the upstream singleton surface.
#
# NOTE: Type predicates (isfinite, isinteger, iszero, isone) are intentionally
# not defined here to avoid dispatch conflicts with the builtin methods.
# Users can define their own predicates for their custom irrational types.

# AbstractIrrational <: Real - base type for irrational values
abstract type AbstractIrrational <: Real end

# Irrational{sym} - parametric struct for specific irrational constants
struct Irrational{sym} <: AbstractIrrational end

Base.:(==)(x::Irrational{s}, y::Irrational{s}) where {s} = true
Base.:(==)(x::AbstractIrrational, y::AbstractIrrational) = false
Base.:(==)(x::AbstractIrrational, y::Real) = false
Base.:(==)(x::Real, y::AbstractIrrational) = false
Base.:(!=)(x::AbstractIrrational, y) = !(x == y)
Base.:(!=)(x, y::AbstractIrrational) = !(x == y)

Base.:(-)(x::AbstractIrrational) = -Float64(x)

Base.:(+)(x::AbstractIrrational, y::AbstractFloat) = typeof(y)(x) + y
Base.:(+)(x::AbstractFloat, y::AbstractIrrational) = x + typeof(x)(y)
Base.:(+)(x::AbstractIrrational, y::Integer) = Float64(x) + Float64(y)
Base.:(+)(x::Integer, y::AbstractIrrational) = Float64(x) + Float64(y)

Base.:(-)(x::AbstractIrrational, y::AbstractFloat) = typeof(y)(x) - y
Base.:(-)(x::AbstractFloat, y::AbstractIrrational) = x - typeof(x)(y)
Base.:(-)(x::AbstractIrrational, y::Integer) = Float64(x) - Float64(y)
Base.:(-)(x::Integer, y::AbstractIrrational) = Float64(x) - Float64(y)
Base.:-(x::AbstractIrrational) = -Float64(x)
float(x::AbstractIrrational) = Float64(x)

Base.:(*)(x::AbstractIrrational, y::AbstractFloat) = typeof(y)(x) * y
Base.:(*)(x::AbstractFloat, y::AbstractIrrational) = x * typeof(x)(y)
Base.:(*)(x::AbstractIrrational, y::Integer) = Float64(x) * Float64(y)
Base.:(*)(x::Integer, y::AbstractIrrational) = Float64(x) * Float64(y)

Base.:(/)(x::AbstractIrrational, y::AbstractFloat) = typeof(y)(x) / y
Base.:(/)(x::AbstractFloat, y::AbstractIrrational) = x / typeof(x)(y)
Base.:(/)(x::AbstractIrrational, y::Integer) = Float64(x) / Float64(y)
Base.:(/)(x::Integer, y::AbstractIrrational) = Float64(x) / Float64(y)

Base.:(^)(x::AbstractIrrational, y::Integer) = Float64(x) ^ Float64(y)

# BigInt partners must promote to BigFloat (current precision), not Float64.
# The generic `::Integer` methods above force Float64; BigInt is the only Integer
# whose promotion with an Irrational is not Float64 (promote_type(Float64, BigInt)
# === BigFloat), so route it explicitly to preserve precision (Issue #9341/#9317).
# min/max likewise (Issue #9384: the `::Integer` min/max methods below would
# otherwise degrade `min(big(1), pi)` to Float64; upstream returns BigFloat).
# (BigFloat partners are already handled by the `::AbstractFloat` methods above.)
Base.:(+)(x::AbstractIrrational, y::BigInt) = BigFloat(x) + BigFloat(y)
Base.:(+)(x::BigInt, y::AbstractIrrational) = BigFloat(x) + BigFloat(y)
Base.:(-)(x::AbstractIrrational, y::BigInt) = BigFloat(x) - BigFloat(y)
Base.:(-)(x::BigInt, y::AbstractIrrational) = BigFloat(x) - BigFloat(y)
Base.:(*)(x::AbstractIrrational, y::BigInt) = BigFloat(x) * BigFloat(y)
Base.:(*)(x::BigInt, y::AbstractIrrational) = BigFloat(x) * BigFloat(y)
Base.:(/)(x::AbstractIrrational, y::BigInt) = BigFloat(x) / BigFloat(y)
Base.:(/)(x::BigInt, y::AbstractIrrational) = BigFloat(x) / BigFloat(y)
Base.min(x::AbstractIrrational, y::BigInt) = min(BigFloat(x), BigFloat(y))
Base.min(x::BigInt, y::AbstractIrrational) = min(BigFloat(x), BigFloat(y))
Base.max(x::AbstractIrrational, y::BigInt) = max(BigFloat(x), BigFloat(y))
Base.max(x::BigInt, y::AbstractIrrational) = max(BigFloat(x), BigFloat(y))

Base.abs(x::AbstractIrrational) = x
Base.abs2(x::AbstractIrrational) = Float64(x) * Float64(x)

# =============================================================================
# Promotion rules (mirrors julia/base/irrationals.jl)
# =============================================================================
# An Irrational has no fixed machine representation, so it promotes to the
# float type of its partner: Float16/Float32 keep their width, every other Real
# widens through Float64 (so Int -> Float64, BigInt/BigFloat -> BigFloat).
# Without these, promote_type(Irrational, Float64) fell back to typejoin ===
# Real and operations forced Float64 (Issue #9341).
promote_rule(::Type{<:AbstractIrrational}, ::Type{Float16}) = Float16
promote_rule(::Type{<:AbstractIrrational}, ::Type{Float32}) = Float32
promote_rule(::Type{<:AbstractIrrational}, ::Type{<:AbstractIrrational}) = Float64
promote_rule(::Type{<:AbstractIrrational}, ::Type{T}) where {T<:Real} = promote_type(Float64, T)

function Base._isapprox_scalar_f64(xx::Float64, yy::Float64, rtol, atol)
    diff = sub_float(xx, yy)
    if lt_float(diff, 0.0)
        diff = sub_float(0.0, diff)
    end
    ax = xx
    if lt_float(ax, 0.0)
        ax = sub_float(0.0, ax)
    end
    ay = yy
    if lt_float(ay, 0.0)
        ay = sub_float(0.0, ay)
    end
    scale = ax
    if lt_float(scale, ay)
        scale = ay
    end
    tol = mul_float(Float64(rtol), scale)
    aa = Float64(atol)
    if lt_float(tol, aa)
        tol = aa
    end
    return le_float(diff, tol)
end

function Base.isapprox(x::Float64, y::AbstractIrrational)
    return _isapprox_scalar_f64(x, Float64(y), 1.4901161193847656e-8, 0.0)
end

function Base.isapprox(x::AbstractIrrational, y::Float64)
    return _isapprox_scalar_f64(Float64(x), y, 1.4901161193847656e-8, 0.0)
end

function Base.isapprox(x::AbstractIrrational, y::AbstractIrrational)
    return _isapprox_scalar_f64(Float64(x), Float64(y), 1.4901161193847656e-8, 0.0)
end

function Base.isapprox(x, y::AbstractIrrational)
    return _isapprox_scalar_f64(Float64(x), Float64(y), 1.4901161193847656e-8, 0.0)
end

function Base.isapprox(x::AbstractIrrational, y)
    return _isapprox_scalar_f64(Float64(x), Float64(y), 1.4901161193847656e-8, 0.0)
end

Base.min(x::AbstractIrrational, y::AbstractFloat) = min(typeof(y)(x), y)
Base.min(x::AbstractFloat, y::AbstractIrrational) = min(x, typeof(x)(y))
Base.min(x::AbstractIrrational, y::Integer) = min(Float64(x), Float64(y))
Base.min(x::Integer, y::AbstractIrrational) = min(Float64(x), Float64(y))
Base.max(x::AbstractIrrational, y::AbstractFloat) = max(typeof(y)(x), y)
Base.max(x::AbstractFloat, y::AbstractIrrational) = max(x, typeof(x)(y))
Base.max(x::AbstractIrrational, y::Integer) = max(Float64(x), Float64(y))
Base.max(x::Integer, y::AbstractIrrational) = max(Float64(x), Float64(y))

function Base._isapprox_scalar(x::Real, y::AbstractIrrational, rtol, atol)
    return _isapprox_scalar_f64(Float64(x), Float64(y), rtol, atol)
end

function Base._isapprox_scalar(x::AbstractIrrational, y::Real, rtol, atol)
    return _isapprox_scalar_f64(Float64(x), Float64(y), rtol, atol)
end

sin(x::AbstractIrrational) = sin(Float64(x))
cos(x::AbstractIrrational) = cos(Float64(x))
tan(x::AbstractIrrational) = tan(Float64(x))
log(x::AbstractIrrational) = log(Float64(x))

# Exact special-case values at π (mirrors julia/base/mathconstants.jl). Float64(π)
# is not exactly π, so sin(Float64(π)) === 1.22e-16 rather than 0.0; upstream
# hard-codes the mathematically exact results (Issue #9341).
sin(::Irrational{:π}) = 0.0
cos(::Irrational{:π}) = -1.0
tan(::Irrational{:π}) = 0.0
big(x::AbstractIrrational) = BigFloat(x)
big(::Type{<:AbstractIrrational}) = BigFloat
