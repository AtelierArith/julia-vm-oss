module Quaternions

# Minimal Quaternions.jl compatibility package for Rotations.jl support
# (Issue #7472, parent #7434).  Implements only the upstream API that
# Rotations.jl's `QuatRotation` / exponential maps / `slerp` rely on:
# the `Quaternion{T}` value type with fields `(s, v1, v2, v3)`, `real`,
# `imag_part`, conjugation, norm/normalisation, multiplication, `exp`/`log`,
# and `slerp`.  Wider Quaternions.jl surface (broadcasting, rand, dual
# quaternions, …) is intentionally deferred; see docs/vm/ROTATIONS.md.

using LinearAlgebra

export Quaternion, QuaternionF64, QuaternionF32, quat, imag_part, slerp

"""
    Quaternion{T<:Real} <: Number

A quaternion `s + v1·i + v2·j + v3·k`, mirroring upstream Quaternions.jl's
field layout `(s, v1, v2, v3)`.
"""
struct Quaternion{T<:Real} <: Number
    s::T
    v1::T
    v2::T
    v3::T
end

const QuaternionF64 = Quaternion{Float64}
const QuaternionF32 = Quaternion{Float32}

# ── Constructors ──────────────────────────────────────────────────────────────
# Same-type case calls the inner constructor directly; mixed types promote first.
# (sjulia does not auto-generate the type-inferring outer constructor, so the
# same-type method is provided explicitly to avoid promote-recursion.)
Quaternion(s::T, v1::T, v2::T, v3::T) where {T<:Real} = Quaternion{T}(s, v1, v2, v3)
Quaternion(s::Real, v1::Real, v2::Real, v3::Real) = Quaternion(promote(s, v1, v2, v3)...)
Quaternion(x::Real) = Quaternion(x, zero(x), zero(x), zero(x))
Quaternion{T}(x::Real) where {T<:Real} = Quaternion{T}(T(x), zero(T), zero(T), zero(T))
Quaternion(s::Real, v::AbstractVector) = Quaternion(s, v[1], v[2], v[3])

quat(args...) = Quaternion(args...)

Base.convert(::Type{Quaternion{T}}, x::Real) where {T} = Quaternion{T}(x)
Base.convert(::Type{Quaternion{T}}, q::Quaternion) where {T} =
    Quaternion{T}(T(q.s), T(q.v1), T(q.v2), T(q.v3))

# ── Accessors ─────────────────────────────────────────────────────────────────
Base.real(q::Quaternion) = q.s
Base.real(::Type{Quaternion{T}}) where {T} = T
imag_part(q::Quaternion) = (q.v1, q.v2, q.v3)
Base.float(q::Quaternion) = Quaternion(float(q.s), float(q.v1), float(q.v2), float(q.v3))

Base.eltype(::Type{Quaternion{T}}) where {T} = T

# ── Equality / predicates ───────────────────────────────────────────────────────
Base.:(==)(q::Quaternion, w::Quaternion) =
    q.s == w.s && q.v1 == w.v1 && q.v2 == w.v2 && q.v3 == w.v3
Base.isreal(q::Quaternion) = iszero(q.v1) && iszero(q.v2) && iszero(q.v3)
Base.iszero(q::Quaternion) = iszero(q.s) && iszero(q.v1) && iszero(q.v2) && iszero(q.v3)

# ── Norm / conjugation ──────────────────────────────────────────────────────────
Base.conj(q::Quaternion) = Quaternion(q.s, -q.v1, -q.v2, -q.v3)
Base.abs2(q::Quaternion) = q.s^2 + q.v1^2 + q.v2^2 + q.v3^2
Base.abs(q::Quaternion) = sqrt(abs2(q))
LinearAlgebra.norm(q::Quaternion) = abs(q)

function LinearAlgebra.normalize(q::Quaternion)
    n = abs(q)
    Quaternion(q.s / n, q.v1 / n, q.v2 / n, q.v3 / n)
end

# upstream `sign(q)` returns the normalised quaternion (q / |q|)
Base.sign(q::Quaternion) = normalize(q)

function Base.inv(q::Quaternion)
    a = abs2(q)
    Quaternion(q.s / a, -q.v1 / a, -q.v2 / a, -q.v3 / a)
end

# ── Arithmetic ──────────────────────────────────────────────────────────────────
Base.:+(q::Quaternion, w::Quaternion) =
    Quaternion(q.s + w.s, q.v1 + w.v1, q.v2 + w.v2, q.v3 + w.v3)
Base.:-(q::Quaternion, w::Quaternion) =
    Quaternion(q.s - w.s, q.v1 - w.v1, q.v2 - w.v2, q.v3 - w.v3)
Base.:-(q::Quaternion) = Quaternion(-q.s, -q.v1, -q.v2, -q.v3)

function Base.:*(q::Quaternion, w::Quaternion)
    s  = q.s * w.s  - q.v1 * w.v1 - q.v2 * w.v2 - q.v3 * w.v3
    v1 = q.s * w.v1 + q.v1 * w.s  + q.v2 * w.v3 - q.v3 * w.v2
    v2 = q.s * w.v2 - q.v1 * w.v3 + q.v2 * w.s  + q.v3 * w.v1
    v3 = q.s * w.v3 + q.v1 * w.v2 - q.v2 * w.v1 + q.v3 * w.s
    Quaternion(s, v1, v2, v3)
end
Base.:*(q::Quaternion, x::Real) = Quaternion(q.s * x, q.v1 * x, q.v2 * x, q.v3 * x)
Base.:*(x::Real, q::Quaternion) = q * x
Base.:/(q::Quaternion, x::Real) = Quaternion(q.s / x, q.v1 / x, q.v2 / x, q.v3 / x)
Base.:/(q::Quaternion, w::Quaternion) = q * inv(w)

# ── Exponential / logarithm (used by Rotations error maps) ───────────────────────
function Base.exp(q::Quaternion)
    es = exp(q.s)
    nv = sqrt(q.v1^2 + q.v2^2 + q.v3^2)
    if iszero(nv)
        return Quaternion(es, zero(es), zero(es), zero(es))
    end
    c = cos(nv)
    sc = sin(nv) / nv
    Quaternion(es * c, es * sc * q.v1, es * sc * q.v2, es * sc * q.v3)
end

function Base.log(q::Quaternion)
    a = abs(q)
    nv = sqrt(q.v1^2 + q.v2^2 + q.v3^2)
    if iszero(nv)
        return Quaternion(log(a), zero(a), zero(a), zero(a))
    end
    th = atan(nv, q.s) / nv
    Quaternion(log(a), th * q.v1, th * q.v2, th * q.v3)
end

# ── Spherical linear interpolation ──────────────────────────────────────────────
function slerp(qa::Quaternion, qb::Quaternion, t::Real)
    a = normalize(qa)
    b = normalize(qb)
    coshalftheta = a.s * b.s + a.v1 * b.v1 + a.v2 * b.v2 + a.v3 * b.v3
    if coshalftheta < 0
        b = -b
        coshalftheta = -coshalftheta
    end
    if coshalftheta > 0.999999
        # nearly parallel: linear interpolation + renormalise
        return normalize(Quaternion(
            a.s + t * (b.s - a.s),
            a.v1 + t * (b.v1 - a.v1),
            a.v2 + t * (b.v2 - a.v2),
            a.v3 + t * (b.v3 - a.v3),
        ))
    end
    halftheta = acos(coshalftheta)
    sinhalftheta = sqrt(1 - coshalftheta^2)
    ratio_a = sin((1 - t) * halftheta) / sinhalftheta
    ratio_b = sin(t * halftheta) / sinhalftheta
    Quaternion(
        a.s * ratio_a + b.s * ratio_b,
        a.v1 * ratio_a + b.v1 * ratio_b,
        a.v2 * ratio_a + b.v2 * ratio_b,
        a.v3 * ratio_a + b.v3 * ratio_b,
    )
end

# ── Display ─────────────────────────────────────────────────────────────────────
function Base.show(io::IO, q::Quaternion)
    print(io, "Quaternion(", q.s, ", ", q.v1, ", ", q.v2, ", ", q.v3, ")")
end

end # module Quaternions
