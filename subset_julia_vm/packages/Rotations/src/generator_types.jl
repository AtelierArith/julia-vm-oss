# Rotation generators — skew-symmetric matrices (adapted from
# extern/Rotations.jl/src/rotation_generator.jl).
#
# Subset scope: the two concrete parametrisation-shaped generators
# `Angle2dGenerator` (2×2) and `RotationVecGenerator` (3×3), plus `skew` and
# `isrotationgenerator`. Upstream's `RotMatrixGenerator` (a dense SMatrix-backed
# generator) and the generator exp/log maps are deferred (see ROTATIONS.md).
#
# Like `Rotation`, `RotationGenerator` does NOT subtype `StaticMatrix` (avoids
# the generic-matmul mis-dispatch, see core_types.jl), and every operation
# shared by both generator types is written as ONE method on the abstract
# `RotationGenerator` with a runtime `isa` branch (#7960).

"""
    abstract type RotationGenerator{N,T}

An `N`-dimensional rotation generator: a skew-symmetric real `N`×`N` matrix.
"""
abstract type RotationGenerator{N,T} end

"""
    Angle2dGenerator{T} <: RotationGenerator{2,T}

The 2×2 skew-symmetric generator `[0 -v; v 0]`.
"""
struct Angle2dGenerator{T} <: RotationGenerator{2,T}
    v::T
end

@inline Angle2dGenerator(r::Number) = Angle2dGenerator{rot_eltype(typeof(r))}(r)

"""
    RotationVecGenerator{T} <: RotationGenerator{3,T}

The 3×3 skew-symmetric generator `[0 -z y; z 0 -x; -y x 0]`.
"""
struct RotationVecGenerator{T} <: RotationGenerator{3,T}
    x::T
    y::T
    z::T
end

@inline function RotationVecGenerator(x::Number, y::Number, z::Number)
    T = promote_type(promote_type(typeof(x), typeof(y)), typeof(z))
    RotationVecGenerator{T}(x, y, z)
end

# Column-major flat tuple of each generator (matches upstream `Tuple`).
function _generator_tuple(r::RotationGenerator)
    if r isa Angle2dGenerator
        z = zero(r.v)
        return (z, r.v, -r.v, z)
    end
    # RotationVecGenerator: [0 -z y; z 0 -x; -y x 0] column-major
    z = zero(r.x)
    return (z, r.z, -r.y, -r.z, z, r.x, r.y, -r.x, z)
end

function Base.Tuple(r::RotationGenerator)
    _generator_tuple(r)
end

@inline Base.getindex(r::RotationGenerator, i::Int) = _generator_tuple(r)[i]
@inline function Base.getindex(r::RotationGenerator, i::Int, j::Int)
    n = (r isa Angle2dGenerator) ? 2 : 3
    _generator_tuple(r)[(j - 1) * n + i]
end

function Base.size(r::RotationGenerator)
    (r isa Angle2dGenerator) ? (2, 2) : (3, 3)
end

function params(r::RotationGenerator)
    if r isa Angle2dGenerator
        return SVector(r.v)
    end
    SVector(r.x, r.y, r.z)
end

# Skew-symmetric algebra: same-type addition / subtraction / negation / scaling
# stay within the generator type (mirrors upstream's specialised methods).
function Base.:+(a::RotationGenerator, b::RotationGenerator)
    if a isa Angle2dGenerator && b isa Angle2dGenerator
        return Angle2dGenerator(a.v + b.v)
    elseif a isa RotationVecGenerator && b isa RotationVecGenerator
        return RotationVecGenerator(a.x + b.x, a.y + b.y, a.z + b.z)
    end
    throw(MethodError(+, (a, b)))
end

function Base.:-(a::RotationGenerator, b::RotationGenerator)
    if a isa Angle2dGenerator && b isa Angle2dGenerator
        return Angle2dGenerator(a.v - b.v)
    elseif a isa RotationVecGenerator && b isa RotationVecGenerator
        return RotationVecGenerator(a.x - b.x, a.y - b.y, a.z - b.z)
    end
    throw(MethodError(-, (a, b)))
end

function Base.:-(r::RotationGenerator)
    if r isa Angle2dGenerator
        return Angle2dGenerator(-r.v)
    end
    RotationVecGenerator(-r.x, -r.y, -r.z)
end

function Base.:*(t::Number, r::RotationGenerator)
    if r isa Angle2dGenerator
        return Angle2dGenerator(t * r.v)
    end
    RotationVecGenerator(t * r.x, t * r.y, t * r.z)
end
@inline Base.:*(r::RotationGenerator, t::Number) = t * r

function Base.:/(r::RotationGenerator, t::Number)
    if r isa Angle2dGenerator
        return Angle2dGenerator(r.v / t)
    end
    RotationVecGenerator(r.x / t, r.y / t, r.z / t)
end

# adjoint / transpose of a skew-symmetric matrix is its negation.
Base.adjoint(r::RotationGenerator) = -r
Base.transpose(r::RotationGenerator) = -r
