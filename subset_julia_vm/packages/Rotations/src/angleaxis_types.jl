# Axis-angle rotation AngleAxis (adapted from
# extern/Rotations.jl/src/angleaxis_types.jl).
#
# Only the struct + a normalising outer constructor + a per-type tuple helper
# live here; the generic operations dispatch from the single `::Rotation`
# methods in core_types.jl (#7960 workaround). Conversions FROM a rotation
# matrix / between AngleAxis and QuatRotation are deferred to Phase 4 (they need
# the quaternion maps).
#
# NB: the axis is renormalised in the OUTER constructor (not a custom inner) and
# the already-normalised values are handed to the default field constructor. A
# custom inner that transforms its arguments is currently mis-dispatched for the
# `outer -> inner(vars)` relay (#8121), so we avoid that pattern.

"""
    AngleAxis{T} <: Rotation{3,T}
    AngleAxis(θ, x, y, z)

A 3×3 rotation by angle `θ` about the axis `[x, y, z]`. The axis is renormalised
to unit length (`x² + y² + z² = 1`). (The upstream `normalize=false` opt-out is
omitted in this MVP.)
"""
struct AngleAxis{T} <: Rotation{3,T}
    theta::T
    axis_x::T
    axis_y::T
    axis_z::T
end

@inline function AngleAxis(theta::Number, x::Number, y::Number, z::Number)
    T0 = promote_type(promote_type(promote_type(typeof(theta), typeof(x)), typeof(y)), typeof(z))
    T = typeof(sin(zero(T0)))
    n = sqrt(x * x + y * y + z * z)
    tt = T(theta)
    ax = T(x / n); ay = T(y / n); az = T(z / n)
    AngleAxis{T}(tt, ax, ay, az)
end

# Rodrigues' rotation formula → column-major flat tuple (matches upstream
# Tuple(::AngleAxis)).
function _angleaxis_tuple(aa::AngleAxis{T}) where {T}
    s, c = sincos(aa.theta)
    c1 = one(T) - c
    x = aa.axis_x; y = aa.axis_y; z = aa.axis_z
    c1x2 = c1 * x^2; c1y2 = c1 * y^2; c1z2 = c1 * z^2
    c1xy = c1 * x * y; c1xz = c1 * x * z; c1yz = c1 * y * z
    sx = s * x; sy = s * y; sz = s * z
    (one(T) - c1y2 - c1z2, c1xy + sz, c1xz - sy,
     c1xy - sz, one(T) - c1x2 - c1z2, c1yz + sx,
     c1xz + sy, c1yz - sx, one(T) - c1x2 - c1y2)
end
