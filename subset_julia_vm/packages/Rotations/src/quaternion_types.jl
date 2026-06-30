# Unit-quaternion rotation QuatRotation (adapted from
# extern/Rotations.jl/src/unitquaternion.jl).
#
# Only the struct + normalising outer constructors live here; every generic
# operation (getindex, Tuple, *, inv, one, rotation_angle, rotation_axis,
# params) dispatches from the single `::Rotation` methods in core_types.jl
# (#7960 workaround), which branch to the quaternion helpers `_to_quat_wxyz` /
# `_quat_matrix_tuple` defined there.
#
# Subset adaptation (#8127): upstream stores a single `q::Quaternion` field and
# exposes `.w/.x/.y/.z` through a `Base.getproperty` overload. sjulia resolves
# field access against the declared fields at compile time and ignores custom
# `getproperty`, so we store the four scalar components `w, x, y, z` directly —
# making `.w/.x/.y/.z` native field accesses (the upstream public interface) —
# and reconstruct a `Quaternion` on demand (e.g. for `slerp`). See WORKAROUNDS.md.
#
# Constructor design (avoids the #8103 / #8121 parametric-constructor traps):
# there is NO custom inner constructor and NO typed `QuatRotation{T}(...)` outer
# constructor. The bare outer constructors compute the element type, optionally
# renormalise to unit norm, and hand four already-`T` scalars to the synthesized
# default field constructor `QuatRotation{T}(::T, ::T, ::T, ::T)`.

"""
    QuatRotation{T} <: Rotation{3,T}
    QuatRotation(w, x, y, z, normalize=true)
    QuatRotation(q::Quaternion, normalize=true)

A 3×3 rotation represented by a unit quaternion `w + x·i + y·j + z·k`
(Hamilton convention). The quaternion is renormalised to unit norm by default.
The components are accessible as `q.w`, `q.x`, `q.y`, `q.z`.
"""
struct QuatRotation{T} <: Rotation{3,T}
    w::T
    x::T
    y::T
    z::T
end

# float element type for the four scalar components.
function _quat_eltype(w, x, y, z)
    T0 = promote_type(promote_type(promote_type(typeof(w), typeof(x)), typeof(y)), typeof(z))
    typeof(float(zero(T0)))
end

@inline function QuatRotation(w::Number, x::Number, y::Number, z::Number,
                              normalize::Bool = true)
    T = _quat_eltype(w, x, y, z)
    wt = T(w); xt = T(x); yt = T(y); zt = T(z)
    if normalize
        n = sqrt(wt * wt + xt * xt + yt * yt + zt * zt)
        return QuatRotation{T}(wt / n, xt / n, yt / n, zt / n)
    end
    QuatRotation{T}(wt, xt, yt, zt)
end

@inline QuatRotation(qq::Quaternion, normalize::Bool = true) =
    QuatRotation(qq.s, qq.v1, qq.v2, qq.v3, normalize)

# Back-conversion to a raw Quaternion (upstream `Quaternions.Quaternion(::QuatRotation)`).
Quaternions.Quaternion(q::QuatRotation) = Quaternion(q.w, q.x, q.y, q.z)

# Spherical linear interpolation between two unit-quaternion rotations. Extends
# the `slerp` generic the Quaternions package provides for raw `Quaternion`s.
Quaternions.slerp(q1::QuatRotation, q2::QuatRotation, t::Real) =
    QuatRotation(slerp(Quaternion(q1.w, q1.x, q1.y, q1.z),
                       Quaternion(q2.w, q2.x, q2.y, q2.z), t))
