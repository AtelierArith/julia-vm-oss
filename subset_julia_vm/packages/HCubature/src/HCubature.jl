module HCubature

using StaticArrays, LinearAlgebra
import Combinatorics, DataStructures, QuadGK

export hcubature, hquadrature, hcubature_buffer, hcubature_count, hcubature_print

include("genz-malik.jl")
include("gauss-kronrod.jl")

struct Box
    a
    b
    I
    E
    kdiv::Int
end

Base.isless(i::Box, j::Box) = isless(i.E, j.E)

struct Trivial end

function (::Trivial)(f, a::SVector{0}, b::SVector{0}, norm)
    I = f(a)
    return I, norm(I - I), 1
end

function _eval_rule(::Trivial, f, a::SVector{0}, b::SVector{0}, norm)
    I = f(a)
    return I, norm(I - I), 1
end

countevals(::Trivial) = 1

# Dimension-indexed rule selection. An integer branch is used instead of
# upstream's `cubrule(::Val{n})` dispatch: mixing concrete `Val{0}`/`Val{1}`
# methods with a generic `Val{n}` method risks the dispatch mis-selection
# tracked in Issue #8537.
function cubrule(n::Integer, ::Type{T}) where {T}
    if n == 0
        return Trivial()
    elseif n == 1
        return GaussKronrod(T)
    end
    return GenzMalik(n, T)
end

function _endpoint_float_type(a, b)
    length(a) == 0 && return Float64
    P = typeof(a[1])
    for x in a
        P = promote_type(P, typeof(x))
    end
    for x in b
        P = promote_type(P, typeof(x))
    end
    return float(P)
end

function _endpoint_svector(::Type{F}, a) where {F}
    n = length(a)
    n == 0 && return SVector{0,F}(())
    vals = F[]
    for i in 1:n
        push!(vals, convert(F, a[i]))
    end
    return SVector{n,F}(Tuple(vals))
end

function hcubature_buffer(f, a, b; norm=norm)
    hcubature_buffer_(f, a, b, norm)
end

function hcubature_buffer_(f, a::SVector{N,T}, b::SVector{N,T}, norm) where {N,T}
    rule = cubrule(N, T)
    I, E, _ = _eval_rule(rule, f, a, b, norm)
    firstbox = HCubature.Box(a, b, I, E, 0)
    return DataStructures.BinaryMaxHeap{typeof(firstbox)}()
end

function hcubature_buffer_(f, a::AbstractVector{T}, b::AbstractVector{S}, norm) where {T<:Real,S<:Real}
    length(a) == length(b) || throw(DimensionMismatch("endpoints must have the same length"))
    F = float(promote_type(T, S))
    return hcubature_buffer_(f, _endpoint_svector(F, a), _endpoint_svector(F, b), norm)
end

function hcubature_buffer_(f, a::Tuple, b::Tuple, norm)
    length(a) == length(b) || throw(DimensionMismatch("endpoints must have the same length"))
    F = _endpoint_float_type(a, b)
    hcubature_buffer_(f, _endpoint_svector(F, a), _endpoint_svector(F, b), norm)
end

function _copy_svector_to_vector!(dest, src)
    for i in 1:length(src)
        dest[i] = src[i]
    end
    return dest
end

function hcubature_(f::F, a::SVector{N,T}, b::SVector{N,T}, norm, rtol_, atol, maxevals, initdiv, buf) where {F,N,T<:Real}
    rtol = (rtol_ == 0 && atol == 0) ? sqrt(eps(T)) : rtol_
    (rtol < 0 || atol < 0) && throw(ArgumentError("invalid negative tolerance"))
    maxevals < 0 && throw(ArgumentError("invalid negative maxevals"))
    initdiv < 1 && throw(ArgumentError("initdiv must be positive"))

    rule = cubrule(N, T)
    numevals = evals_per_box = countevals(rule)

    if N == 0
        I, E, _ = _eval_rule(rule, f, a, b, norm)
        return I, E
    end

    delta = (b - a) / initdiv
    b1 = initdiv == 1 ? b : a + delta
    I, E, kdiv = _eval_rule(rule, f, a, b1, norm)
    iszero(prod(delta)) && return I, E

    firstbox = HCubature.Box(a, b1, I, E, kdiv)
    boxes = if buf === nothing
        DataStructures.BinaryMaxHeap{typeof(firstbox)}()
    else
        empty!(buf.valtree)
        buf
    end
    push!(boxes, firstbox)

    if initdiv > 1
        # initial box divided by initdiv along each dimension; the odometer
        # walks the same first-dimension-fastest order as upstream's
        # CartesianIndices loop, skipping the already-added first box
        ma0 = copy(a)
        mb0 = copy(b)
        idx = fill(1, N)
        while true
            k = 1
            while k <= N && idx[k] == initdiv
                idx[k] = 1
                k += 1
            end
            k > N && break
            idx[k] += 1
            for i in 1:N
                ma0[i] = a[i] + (idx[i] - 1) * delta[i]
                mb0[i] = idx[i] == initdiv ? b[i] : a[i] + idx[i] * delta[i]
            end
            lo = SVector(ma0)
            hi = SVector(mb0)
            box = HCubature.Box(lo, hi, _eval_rule(rule, f, lo, hi, norm)...)
            I += box.I
            E += box.E
            numevals += evals_per_box
            push!(boxes, box)
        end
    end

    (E <= max(rtol * norm(I), atol) || numevals >= maxevals) && return I, E

    ma = copy(a)
    mb = copy(b)

    while true
        box = pop!(boxes)
        w = (box.b[box.kdiv] - box.a[box.kdiv]) * T(0.5)

        _copy_svector_to_vector!(ma, box.a)
        ma[box.kdiv] += w
        ap = SVector(ma)

        _copy_svector_to_vector!(mb, box.b)
        mb[box.kdiv] -= w
        bp = SVector(mb)

        box1 = HCubature.Box(ap, box.b, _eval_rule(rule, f, ap, box.b, norm)...)
        box2 = HCubature.Box(box.a, bp, _eval_rule(rule, f, box.a, bp, norm)...)
        push!(boxes, box1)
        push!(boxes, box2)

        I += box1.I + box2.I - box.I
        E += box1.E + box2.E - box.E
        numevals += 2 * evals_per_box

        Inorm = norm(I)
        (E <= max(rtol * Inorm, atol) || numevals >= maxevals || !isfinite(Inorm)) && break
    end

    I = zero(I)
    E = zero(E)
    for i in 1:length(boxes.valtree)
        I += boxes.valtree[i].I
        E += boxes.valtree[i].E
    end
    return I, E
end

function hcubature_(f, a::AbstractVector{T}, b::AbstractVector{S}, norm, rtol, atol, maxevals, initdiv, buf) where {T<:Real,S<:Real}
    length(a) == length(b) || throw(DimensionMismatch("endpoints must have the same length"))
    F = float(promote_type(T, S))
    return hcubature_(f, _endpoint_svector(F, a), _endpoint_svector(F, b), norm, rtol, atol, maxevals, initdiv, buf)
end

function hcubature_(f, a::Tuple, b::Tuple, norm, rtol, atol, maxevals, initdiv, buf)
    length(a) == length(b) || throw(DimensionMismatch("endpoints must have the same length"))
    F = _endpoint_float_type(a, b)
    hcubature_(f, _endpoint_svector(F, a), _endpoint_svector(F, b), norm, rtol, atol, maxevals, initdiv, buf)
end

hcubature(f, a, b; norm=norm, rtol::Real=0, atol::Real=0,
          maxevals::Integer=typemax(Int), initdiv::Integer=1, buffer=nothing) =
    hcubature_(f, a, b, norm, rtol, atol, maxevals, initdiv, buffer)

function hcubature_count(f, a, b; kws...)
    count = Ref(0)
    wrapped = x -> begin
        count[] += 1
        f(x)
    end
    I, E = hcubature(wrapped, a, b; kws...)
    return I, E, count[]
end

function hcubature_print(io::IO, f, a, b; kws...)
    count = Ref(0)
    wrapped = x -> begin
        y = f(x)
        count[] += 1
        println(io, "f($x) = $y")
        y
    end
    I, E = hcubature(wrapped, a, b; kws...)
    return I, E, count[]
end

hcubature_print(f, a, b; kws...) = hcubature_print(stdout, f, a, b; kws...)

function hquadrature(f, a::T, b::S; norm=norm, rtol::Real=0, atol::Real=0,
                     maxevals::Integer=typemax(Int), initdiv::Integer=1, buffer=nothing) where {T<:Real,S<:Real}
    F = float(promote_type(T, S))
    hcubature_(x -> f(x[1]), SVector{1,F}(a), SVector{1,F}(b), norm, rtol, atol, maxevals, initdiv, buffer)
end

end
