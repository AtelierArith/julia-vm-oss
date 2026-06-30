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
Base.abs(x::AbstractIrrational) = x
Base.abs2(x::AbstractIrrational) = Float64(x) * Float64(x)

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
big(x::AbstractIrrational) = BigFloat(x)
big(::Type{<:AbstractIrrational}) = BigFloat
