# Abstract rotation type and 2D rotation matrices (adapted from
# extern/Rotations.jl/src/core_types.jl).
#
# Adaptations to the sjulia subset:
#   * `RotMatrix` is `{N,T}` wrapping the incompletely-parameterized
#     `SMatrix{N,N,T}` (Issue #11432: bundled StaticArrays' `SMatrix` now
#     declares the upstream fourth length parameter `L`, but `RotMatrix`'s own
#     `{N,T}` still drops it, same as upstream); the upstream `@eval` alias
#     loop / `similar_type` machinery is hand-expanded.
#   * #8090: a parametric-struct constructor result must be bound to a local
#     variable before being passed as an argument; only one
#     `RotMatrix{N,T}(...) where {N,T}` constructor may exist; non-typed wraps go
#     through the concrete `RotMatrix{2,Float64}` / `RotMatrix{3,Float64}`.
#   * #7960: sjulia mis-dispatches between sibling concrete methods of a shared
#     generic function (e.g. `*(::RotMatrix,::StaticVector)` vs
#     `*(::Angle2d,::StaticVector)`). Every operation defined for more than one
#     rotation type is therefore written as ONE method on the abstract
#     `Rotation` with a runtime `isa` branch (the same workaround StaticArrays
#     uses for `*`). `isa`/`<:` are reliable here after #8092.
#   * #8103 (B): `Rotation` subtypes NOTHING (upstream spells it
#     `<: StaticMatrix{N,N,T}`). Subtyping `AbstractMatrix`/`StaticMatrix` makes
#     `r * v` mis-select the prelude generic `*(::AbstractMatrix,::AbstractVector)`
#     over the specific `*(::Rotation,::StaticVector)`, and the generic then fails
#     on a missing `size(::Rotation,::Int)`. Standing free keeps the
#     rotation-specific operators unambiguous and the results `SVector`-typed.
#     (`r isa AbstractMatrix` is therefore false in this MVP — see ROTATIONS.md.)

abstract type Rotation{N,T} end

# ── small column-major SMatrix helpers (N ∈ {2,3} for the MVP) ────────────────────
function _identity_smatrix(N::Int, ::Type{T}) where {T}
    o = one(T); z = zero(T)
    if N == 2
        return SMatrix{2,2}((o, z, z, o))
    elseif N == 3
        return SMatrix{3,3}((o, z, z, z, o, z, z, z, o))
    end
    error("Rotations MVP supports identity for N=2,3 only (got N=$(N))")
end

# ── RotMatrix ───────────────────────────────────────────────────────────────────
"""
    RotMatrix{N,T} <: Rotation{N,T}

A statically-sized N×N rotation (orthogonal) matrix wrapping an `SMatrix`.
Orthonormality of the input is *not* checked by the constructor.
"""
struct RotMatrix{N,T} <: Rotation{N,T}
    mat::SMatrix{N,N,T}
end

const RotMatrix2{T} = RotMatrix{2,T}
const RotMatrix3{T} = RotMatrix{3,T}

# Wrap an SMatrix into the matching concrete RotMatrix (Float64 element type;
# use the typed constructor to preserve another T). No generic `where`-method.
function _wrap_rotmatrix(m::SMatrix)
    t = Tuple(m)
    L = length(t)
    if L == 4
        return RotMatrix{2,Float64}(m)
    elseif L == 9
        return RotMatrix{3,Float64}(m)
    end
    throw(DimensionMismatch("RotMatrix supports 2×2 and 3×3 only (got length $(L))."))
end

# square-tuple constructors (column-major flat tuple of length N*N).
function RotMatrix(t::Tuple)
    L = length(t)
    if L == 4
        m = SMatrix{2,2}(t)
        return RotMatrix{2,Float64}(m)
    elseif L == 9
        m = SMatrix{3,3}(t)
        return RotMatrix{3,Float64}(m)
    end
    throw(DimensionMismatch("The length of input tuple $(t) must be a square number (4 or 9 in this MVP)."))
end

RotMatrix(m::SMatrix) = _wrap_rotmatrix(m)

# 2D angle constructors. Column-major flat tuple of [c -s; s c] is (c, s, -s, c).
@inline function RotMatrix(theta::Number)
    s, c = sincos(theta)
    m = SMatrix{2,2}((c, s, -s, c))
    RotMatrix{2,Float64}(m)
end
# The single reserved `where {N,T}` constructor: typed angle (only N=2 meaningful).
function RotMatrix{N,T}(theta::Number) where {N,T}
    cc = T(cos(theta)); ss = T(sin(theta)); nss = T(-sin(theta))
    m = SMatrix{2,2}((cc, ss, nss, cc))
    RotMatrix{N,T}(m)
end

function Base.one(::Type{RotMatrix2{T}}) where {T}
    m = _identity_smatrix(2, T)
    RotMatrix{2,T}(m)
end
function Base.one(::Type{RotMatrix3{T}}) where {T}
    m = _identity_smatrix(3, T)
    RotMatrix{3,T}(m)
end

# ── Angle2d ─────────────────────────────────────────────────────────────────────
"""
    Angle2d{T} <: Rotation{2,T}

A 2×2 rotation parametrised by an angle `theta`; entries are computed on the fly.
"""
struct Angle2d{T} <: Rotation{2,T}
    theta::T
    Angle2d{T}(theta::Number) where {T} = new{T}(T(theta))
end

@inline Angle2d(theta::Number) = Angle2d{rot_eltype(typeof(theta))}(theta)
Angle2d(r::Rotation{2}) = Angle2d(rotation_angle(r))

Base.one(::Type{Angle2d{T}}) where {T} = Angle2d{T}(zero(T))

function RotMatrix(r::Angle2d)
    s, c = sincos(r.theta)
    m = SMatrix{2,2}((c, s, -s, c))
    RotMatrix{2,Float64}(m)
end

# ── Operations: ONE method per generic function, branching on the concrete type
#    at run time to dodge the sibling-method mis-dispatch (#7960). ───────────────

# Column-major flat tuple of the single-axis 3-D rotations (matches upstream
# `Tuple(RotX/Y/Z)`). Forward-references RotX/RotY/RotZ (defined in
# euler_types.jl, included after this file) — method bodies resolve names at run
# time, so this is fine.
function _axis_rot_tuple(r::Rotation)
    s, c = sincos(r.theta)
    o = one(s); z = zero(s)
    if r isa RotX
        return (o, z, z, z, c, s, z, -s, c)
    elseif r isa RotY
        return (c, z, -s, z, o, z, s, z, c)
    else # RotZ
        return (c, s, z, -s, c, z, z, z, o)
    end
end

_is_axis_rot(r::Rotation) = (r isa RotX) || (r isa RotY) || (r isa RotZ)

# Convert any rotation to its `RotMatrix` form (for composition / generic ops).
function _as_rotmatrix(r::Rotation)
    if r isa RotMatrix
        return r
    end
    t = Tuple(r)
    RotMatrix(t)
end

# ── quaternion-family helpers (QuatRotation, RotationVec, RodriguesParam, MRP) ──
# All four 3-D parametrisations realise their rotation matrix through a unit
# quaternion, exactly as upstream does. `_to_quat_wxyz` returns the (w,x,y,z)
# components and `_quat_matrix_tuple` expands them into the column-major 9-tuple
# (identical to upstream `Tuple(::QuatRotation)`). Types referenced here are
# defined in quaternion_types.jl / param3_types.jl, included after this file;
# method bodies resolve names at run time so the forward reference is fine.
_is_quat_family(r::Rotation) =
    (r isa QuatRotation) || (r isa RotationVec) || (r isa RodriguesParam) || (r isa MRP)

function _quat_matrix_tuple(w, x, y, z)
    ww = w * w; xx = x * x; yy = y * y; zz = z * z
    xy = x * y; zw = w * z; xz = x * z; yw = y * w; yz = y * z; xw = w * x
    (ww + xx - yy - zz,
     2 * (xy + zw),
     2 * (xz - yw),
     2 * (xy - zw),
     ww - xx + yy - zz,
     2 * (yz + xw),
     2 * (xz + yw),
     2 * (yz - xw),
     ww - xx - yy + zz)
end

# (w,x,y,z) unit-quaternion components of any supported 3-D rotation.
function _to_quat_wxyz(r::Rotation)
    if r isa QuatRotation
        return (r.w, r.x, r.y, r.z)
    elseif r isa RodriguesParam
        x = r.x; y = r.y; z = r.z
        M = 1 / sqrt(1 + x * x + y * y + z * z)
        return (M, M * x, M * y, M * z)
    elseif r isa MRP
        x = r.x; y = r.y; z = r.z
        n2 = x * x + y * y + z * z
        d = 1 + n2
        M = 2 / d
        return ((1 - n2) / d, M * x, M * y, M * z)
    elseif r isa RotationVec
        sx = r.sx; sy = r.sy; sz = r.sz
        theta = sqrt(sx * sx + sy * sy + sz * sz)
        if theta < sqrt(eps(typeof(float(theta))))
            sc = sinc(theta / π / 2) / 2  # gracefully handles theta = 0
            return (cos(theta / 2), sc * sx, sc * sy, sc * sz)
        end
        s, c = sincos(theta / 2)
        st = s / theta
        return (c, st * sx, st * sy, st * sz)
    elseif r isa AngleAxis
        s, c = sincos(r.theta / 2)
        return (c, s * r.axis_x, s * r.axis_y, s * r.axis_z)
    elseif r isa RotX
        s, c = sincos(r.theta / 2)
        return (c, s, zero(s), zero(s))
    elseif r isa RotY
        s, c = sincos(r.theta / 2)
        return (c, zero(s), s, zero(s))
    elseif r isa RotZ
        s, c = sincos(r.theta / 2)
        return (c, zero(s), zero(s), s)
    end
    error("_to_quat_wxyz: unsupported rotation type")
end

Base.adjoint(r::Rotation) = inv(r)
Base.transpose(r::Rotation) = inv(r)
function Base.size(r::Rotation)
    (r isa Angle2d || (r isa RotMatrix && length(Tuple(r.mat)) == 4)) ? (2, 2) : (3, 3)
end
function Base.one(r::Rotation)
    if r isa Angle2d
        return Angle2d{typeof(r.theta)}(zero(r.theta))
    elseif r isa RotX
        return RotX{typeof(r.theta)}(zero(r.theta))
    elseif r isa RotY
        return RotY{typeof(r.theta)}(zero(r.theta))
    elseif r isa RotZ
        return RotZ{typeof(r.theta)}(zero(r.theta))
    elseif r isa AngleAxis
        return AngleAxis(zero(r.theta), one(r.theta), zero(r.theta), zero(r.theta))
    elseif r isa QuatRotation
        o = one(r.w); z = zero(r.w)
        return QuatRotation(o, z, z, z, false)
    elseif r isa RotationVec
        z = zero(r.sx)
        return RotationVec(z, z, z)
    elseif r isa RodriguesParam
        z = zero(r.x)
        return RodriguesParam(z, z, z)
    elseif r isa MRP
        z = zero(r.x)
        return MRP(z, z, z)
    end
    m = _identity_smatrix(length(Tuple(r.mat)) == 4 ? 2 : 3, eltype(r.mat))
    RotMatrix(m)
end

@inline function Base.getindex(r::Rotation, i::Int)
    if r isa Angle2d
        s, c = sincos(r.theta)
        if i == 1
            return c
        elseif i == 2
            return s
        elseif i == 3
            return -s
        elseif i == 4
            return c
        else
            throw(BoundsError(r, i))
        end
    elseif _is_axis_rot(r)
        return _axis_rot_tuple(r)[i]
    elseif r isa AngleAxis
        return _angleaxis_tuple(r)[i]
    elseif _is_quat_family(r)
        w, x, y, z = _to_quat_wxyz(r)
        return _quat_matrix_tuple(w, x, y, z)[i]
    end
    return r.mat[i]
end
@inline function Base.getindex(r::Rotation, i::Int, j::Int)
    if r isa Angle2d
        return r[(j - 1) * 2 + i]
    elseif _is_axis_rot(r)
        return _axis_rot_tuple(r)[(j - 1) * 3 + i]
    elseif r isa AngleAxis
        return _angleaxis_tuple(r)[(j - 1) * 3 + i]
    elseif _is_quat_family(r)
        w, x, y, z = _to_quat_wxyz(r)
        return _quat_matrix_tuple(w, x, y, z)[(j - 1) * 3 + i]
    end
    return r.mat[i, j]
end

function Base.Tuple(r::Rotation)
    if r isa Angle2d
        s, c = sincos(r.theta)
        return (c, s, -s, c)
    elseif _is_axis_rot(r)
        return _axis_rot_tuple(r)
    elseif r isa AngleAxis
        return _angleaxis_tuple(r)
    elseif _is_quat_family(r)
        w, x, y, z = _to_quat_wxyz(r)
        return _quat_matrix_tuple(w, x, y, z)
    end
    # Build via linear getindex rather than `Tuple(r.mat)`: calling the heavily
    # overloaded `Tuple` on the inner SMatrix from inside `Tuple(::Rotation)`
    # trips sjulia's call-site dispatch cache (#7960) for some element types.
    m = r.mat
    n = length(m)
    if n == 4
        return (m[1], m[2], m[3], m[4])
    elseif n == 9
        return (m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9])
    end
    throw(DimensionMismatch("Tuple(Rotation) supports 2×2/3×3 only"))
end

function params(r::Rotation)
    if r isa Angle2d || _is_axis_rot(r)
        v = SVector(r.theta)
        return v
    elseif r isa AngleAxis
        v = SVector(r.theta, r.axis_x, r.axis_y, r.axis_z)
        return v
    elseif r isa QuatRotation
        return SVector(r.w, r.x, r.y, r.z)
    elseif r isa RotationVec
        return SVector(r.sx, r.sy, r.sz)
    elseif r isa RodriguesParam || r isa MRP
        return SVector(r.x, r.y, r.z)
    end
    t = Tuple(r)
    SVector(t)
end

function Base.inv(r::Rotation)
    if r isa Angle2d
        return Angle2d(-r.theta)
    elseif r isa RotX
        return RotX(-r.theta)
    elseif r isa RotY
        return RotY(-r.theta)
    elseif r isa RotZ
        return RotZ(-r.theta)
    elseif r isa AngleAxis
        return AngleAxis(-r.theta, r.axis_x, r.axis_y, r.axis_z)
    elseif r isa QuatRotation
        return QuatRotation(r.w, -r.x, -r.y, -r.z, false)
    elseif r isa RotationVec
        return RotationVec(-r.sx, -r.sy, -r.sz)
    elseif r isa RodriguesParam
        return RodriguesParam(-r.x, -r.y, -r.z)
    elseif r isa MRP
        return MRP(-r.x, -r.y, -r.z)
    end
    t = Tuple(r.mat)
    L = length(t)
    if L == 4
        return RotMatrix((t[1], t[3], t[2], t[4]))            # transpose of 2×2
    elseif L == 9
        return RotMatrix((t[1], t[4], t[7], t[2], t[5], t[8], t[3], t[6], t[9]))
    end
    error("inv(Rotation) supports 2×2/3×3 only")
end

# Vector rotation AND composition share ONE `*` method on `Rotation`. The
# right-hand operand is left untyped and branched on at run time, because under
# the heavily-overloaded `Base.*` sjulia mis-dispatches between two
# `*(::Rotation, ...)` methods that differ only in the second argument type
# (#7960). One method, one entry in the `*` table.
function Base.:*(r::Rotation, x)
    if x isa StaticVector
        if r isa Angle2d
            check_length(x, 2)
            a = x[1]; b = x[2]
            s, c = sincos(r.theta)
            return SVector(c * a - s * b, s * a + c * b)
        elseif _is_axis_rot(r)
            check_length(x, 3)
            s, c = sincos(r.theta)
            if r isa RotX
                return SVector(x[1], x[2] * c - x[3] * s, x[3] * c + x[2] * s)
            elseif r isa RotY
                return SVector(x[1] * c + x[3] * s, x[2], x[3] * c - x[1] * s)
            else # RotZ
                return SVector(x[1] * c - x[2] * s, x[2] * c + x[1] * s, x[3])
            end
        end
        # RotMatrix and the matrix-via-Tuple types (AngleAxis, ...). Multiply by
        # the column-major flat tuple directly (m[i,j] = t[(j-1)*M + i]) instead
        # of `r.mat * x`: a boxed 3×3 SMatrix read out of a struct field loses
        # its element-type parameter in dispatch (#8090), so the
        # `*(::StaticMatrix,::StaticVector)` matvec misses.
        t = Tuple(r)
        L = length(t)
        if L == 4
            check_length(x, 2)
            return SVector(t[1] * x[1] + t[3] * x[2],
                           t[2] * x[1] + t[4] * x[2])
        elseif L == 9
            check_length(x, 3)
            return SVector(t[1] * x[1] + t[4] * x[2] + t[7] * x[3],
                           t[2] * x[1] + t[5] * x[2] + t[8] * x[3],
                           t[3] * x[1] + t[6] * x[2] + t[9] * x[3])
        end
        throw(DimensionMismatch("rotation * vector supports 2×2/3×3 only"))
    elseif x isa Rotation
        if r isa Angle2d && x isa Angle2d
            return Angle2d(r.theta + x.theta)
        end
        m1 = _as_rotmatrix(r)
        m2 = _as_rotmatrix(x)
        p = m1.mat * m2.mat
        return _wrap_rotmatrix(p)
    end
    throw(MethodError(*, (r, x)))
end

@inline function Base.:/(r1::Rotation, r2::Rotation)
    ir = inv(r2)
    r1 * ir
end
@inline function Base.:\(r1::Rotation, r2::Rotation)
    ir = inv(r1)
    ir * r2
end
@inline function Base.:\(r::Rotation, v::StaticVector)
    ir = inv(r)
    ir * v
end

# ── rotation_angle / rotation_axis / isrotation ─────────────────────────────────
function rotation_angle(r::Rotation)
    if r isa Angle2d || _is_axis_rot(r) || r isa AngleAxis
        return r.theta
    elseif r isa RotationVec
        return sqrt(r.sx * r.sx + r.sy * r.sy + r.sz * r.sz)
    elseif _is_quat_family(r)        # QuatRotation, RodriguesParam, MRP
        w, x, y, z = _to_quat_wxyz(r)
        return 2 * atan(sqrt(x * x + y * y + z * z), w)
    end
    atan(r[2, 1], r[1, 1])
end

"""
    rotation_axis(r) -> SVector{3}

The (unit) axis of a 3-D rotation. Defined for the single-axis types in this
phase; general `Rotation{3}` axis extraction arrives with `AngleAxis`.
"""
function rotation_axis(r::Rotation)
    if r isa RotX
        return SVector(one(r.theta), zero(r.theta), zero(r.theta))
    elseif r isa RotY
        return SVector(zero(r.theta), one(r.theta), zero(r.theta))
    elseif r isa RotZ
        return SVector(zero(r.theta), zero(r.theta), one(r.theta))
    elseif r isa AngleAxis
        return SVector(r.axis_x, r.axis_y, r.axis_z)
    elseif r isa RotationVec
        theta = sqrt(r.sx * r.sx + r.sy * r.sy + r.sz * r.sz)
        return theta > 0 ? SVector(r.sx / theta, r.sy / theta, r.sz / theta) :
               SVector(one(theta), zero(theta), zero(theta))
    elseif _is_quat_family(r)        # QuatRotation, RodriguesParam, MRP
        w, x, y, z = _to_quat_wxyz(r)
        s2 = sqrt(x * x + y * y + z * z)
        return s2 > 0 ? SVector(x / s2, y / s2, z / s2) :
               SVector(one(s2), zero(s2), zero(s2))
    end
    error("rotation_axis is only defined for RotX/RotY/RotZ, AngleAxis and the quaternion-family types in this MVP phase")
end

"""
    isrotation(r) -> Bool

`true` when `r` is (numerically) a proper 2-D rotation: `r * rᵀ ≈ I` and
`det(r) > 0`.  (3-D `isrotation` is added in Phase 3.)
"""
function isrotation(r::Rotation)
    a = r[1, 1]; b = r[1, 2]; c = r[2, 1]; d = r[2, 2]
    tol = 1000 * eps(typeof(float(a)))
    det = a * d - b * c
    orth = abs(a * a + c * c - 1) + abs(b * b + d * d - 1) + abs(a * b + c * d)
    orth <= tol && det > 0
end
