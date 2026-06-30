# Three-parameter rotation parametrisations RotationVec, RodriguesParam and MRP
# (adapted from extern/Rotations.jl/src/{angleaxis_types,rodrigues_params,mrps}.jl).
#
# Upstream realises every one of these as a unit quaternion and reads its
# rotation matrix back from `QuatRotation`. We keep that structure: the structs
# and outer constructors live here, while the matrix/operation surface
# dispatches from the single `::Rotation` methods in core_types.jl, which obtain
# the (w,x,y,z) quaternion components via `_to_quat_wxyz` and expand them with
# `_quat_matrix_tuple`.
#
# Only the bare outer constructor (no type parameters) is defined; the typed
# `T{P...}(fields...)` form is the synthesized default field constructor, whose
# numeric arguments convert to the numeric fields without tripping #8103.

"""
    RotationVec{T} <: Rotation{3,T}
    RotationVec(sx, sy, sz)

Rotation-vector (exponential-coordinate) parametrisation: the direction of
`[sx, sy, sz]` is the rotation axis and its norm is the rotation angle.
"""
struct RotationVec{T} <: Rotation{3,T}
    sx::T
    sy::T
    sz::T
end

@inline function RotationVec(x::Number, y::Number, z::Number)
    T = promote_type(promote_type(typeof(x), typeof(y)), typeof(z))
    RotationVec{T}(x, y, z)
end

"""
    RodriguesParam{T} <: Rotation{3,T}
    RodriguesParam(x, y, z)

Rodrigues (Gibbs) parameters `g = tan(θ/2)·axis`; a three-parameter
parametrisation with a singularity at 180°.
"""
struct RodriguesParam{T} <: Rotation{3,T}
    x::T
    y::T
    z::T
end

@inline function RodriguesParam(x::Number, y::Number, z::Number)
    T = promote_type(promote_type(typeof(x), typeof(y)), typeof(z))
    RodriguesParam{T}(x, y, z)
end

"""
    MRP{T} <: Rotation{3,T}
    MRP(x, y, z)

Modified Rodrigues Parameters: a stereographic projection of the unit
quaternion sphere with a singularity at θ = ±360°.
"""
struct MRP{T} <: Rotation{3,T}
    x::T
    y::T
    z::T
end

@inline function MRP(x::Number, y::Number, z::Number)
    T = promote_type(promote_type(typeof(x), typeof(y)), typeof(z))
    MRP{T}(x, y, z)
end
