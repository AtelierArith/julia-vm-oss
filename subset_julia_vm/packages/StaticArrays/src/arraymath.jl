# Static array arithmetic (Issue #7461).
#
# The generic tail of each method indexes through `getindex` / `size` / `length`
# so it stays agnostic to the column-major internal layout SMatrix uses (see
# indexing.jl, Issue #8084). The result is always built as an `SVector` (stack-shaped, fixed
# length) to match upstream StaticArrays, which returns a static vector from
# matrix*vector and from vector +/-.
#
# Performance (Issue #7956). Two things dominate static-array arithmetic in the
# VM, and both are addressed here:
#
#  1. `size`/`length` reflection. Reading dims via `typeof(x).parameters[i]` is
#     ~20x more expensive than a where-clause destructure; the fast value paths
#     for that live in indexing.jl. Every method below still pays it only once
#     (the size/length guard), not per element.
#  2. Per-element typed `getindex`. `A[i,j]` re-matches `SMatrix{M,N,T}` and
#     recomputes the column-major offset on every call (~4x the cost of a raw
#     tuple index). For the small fixed sizes that dominate real workloads (the
#     2x2 / 3x3 / 4x4 IFS affine kernel, #7461/#7949) we take a hand-unrolled
#     fast path: grab the backing `.data` tuple once and index it directly
#     (column-major: element (i,j) of an MxN matrix is `data[(j-1)*M + i]`,
#     Issue #8084), then hand the typed `SVector{N,T}` inner constructor a single
#     tuple literal — no per-element dispatch, no intermediate `Vector`, no splat.
#
# These fast paths are selected by a runtime `size`/`length` branch rather than
# by specializing on `StaticMatrix{2,2,T}` value parameters, because sjulia
# mis-dispatches methods specialized on an abstract type with multiple integer
# value parameters when called through a concrete subtype (Issue #7960): it
# would silently route a 2x2 argument to the 3x3 method. The single-method
# runtime branch sidesteps that until #7960 is fixed.

# Matrix * vector: (A * v)[i] = sum_j A[i, j] * v[j].
function Base.:*(A::StaticMatrix, v::StaticVector)
    s = size(A)
    m = s[1]
    n = s[2]
    n == length(v) || error("DimensionMismatch: matrix has dimensions $(s) but vector has length $(length(v))")
    # Fast paths (Issue #7956): raw column-major `.data` indexing (Issue #8084),
    # direct tuple build. Column-major: A[i,j] = data[(j-1)*M + i].
    if m == 2 && n == 2
        d = A.data
        vd = v.data
        r1 = d[1] * vd[1] + d[3] * vd[2]
        r2 = d[2] * vd[1] + d[4] * vd[2]
        return SVector{2,typeof(r1)}((r1, r2))
    elseif m == 3 && n == 3
        d = A.data
        vd = v.data
        r1 = d[1] * vd[1] + d[4] * vd[2] + d[7] * vd[3]
        r2 = d[2] * vd[1] + d[5] * vd[2] + d[8] * vd[3]
        r3 = d[3] * vd[1] + d[6] * vd[2] + d[9] * vd[3]
        return SVector{3,typeof(r1)}((r1, r2, r3))
    elseif m == 4 && n == 4
        d = A.data
        vd = v.data
        r1 = d[1] * vd[1] + d[5] * vd[2] + d[9]  * vd[3] + d[13] * vd[4]
        r2 = d[2] * vd[1] + d[6] * vd[2] + d[10] * vd[3] + d[14] * vd[4]
        r3 = d[3] * vd[1] + d[7] * vd[2] + d[11] * vd[3] + d[15] * vd[4]
        r4 = d[4] * vd[1] + d[8] * vd[2] + d[12] * vd[3] + d[16] * vd[4]
        return SVector{4,typeof(r1)}((r1, r2, r3, r4))
    end
    vals = []
    for i in 1:m
        acc = A[i, 1] * v[1]
        for j in 2:n
            acc = acc + A[i, j] * v[j]
        end
        push!(vals, acc)
    end
    return SVector(vals...)
end

# Elementwise vector +/- producing a static vector.
function Base.:+(a::StaticVector, b::StaticVector)
    n = length(a)
    n == length(b) || error("DimensionMismatch: vectors have lengths $(n) and $(length(b))")
    if n == 2
        ad = a.data
        bd = b.data
        r1 = ad[1] + bd[1]
        r2 = ad[2] + bd[2]
        return SVector{2,typeof(r1)}((r1, r2))
    elseif n == 3
        ad = a.data
        bd = b.data
        r1 = ad[1] + bd[1]
        r2 = ad[2] + bd[2]
        r3 = ad[3] + bd[3]
        return SVector{3,typeof(r1)}((r1, r2, r3))
    elseif n == 4
        ad = a.data
        bd = b.data
        r1 = ad[1] + bd[1]
        r2 = ad[2] + bd[2]
        r3 = ad[3] + bd[3]
        r4 = ad[4] + bd[4]
        return SVector{4,typeof(r1)}((r1, r2, r3, r4))
    end
    vals = []
    for i in 1:n
        push!(vals, a[i] + b[i])
    end
    return SVector(vals...)
end

function Base.:-(a::StaticVector, b::StaticVector)
    n = length(a)
    n == length(b) || error("DimensionMismatch: vectors have lengths $(n) and $(length(b))")
    if n == 2
        ad = a.data
        bd = b.data
        r1 = ad[1] - bd[1]
        r2 = ad[2] - bd[2]
        return SVector{2,typeof(r1)}((r1, r2))
    elseif n == 3
        ad = a.data
        bd = b.data
        r1 = ad[1] - bd[1]
        r2 = ad[2] - bd[2]
        r3 = ad[3] - bd[3]
        return SVector{3,typeof(r1)}((r1, r2, r3))
    elseif n == 4
        ad = a.data
        bd = b.data
        r1 = ad[1] - bd[1]
        r2 = ad[2] - bd[2]
        r3 = ad[3] - bd[3]
        r4 = ad[4] - bd[4]
        return SVector{4,typeof(r1)}((r1, r2, r3, r4))
    end
    vals = []
    for i in 1:n
        push!(vals, a[i] - b[i])
    end
    return SVector(vals...)
end

# Scalar * vector and vector * scalar.
function Base.:*(c::Number, v::StaticVector)
    n = length(v)
    if n == 2
        vd = v.data
        r1 = c * vd[1]
        r2 = c * vd[2]
        return SVector{2,typeof(r1)}((r1, r2))
    elseif n == 3
        vd = v.data
        r1 = c * vd[1]
        r2 = c * vd[2]
        r3 = c * vd[3]
        return SVector{3,typeof(r1)}((r1, r2, r3))
    elseif n == 4
        vd = v.data
        r1 = c * vd[1]
        r2 = c * vd[2]
        r3 = c * vd[3]
        r4 = c * vd[4]
        return SVector{4,typeof(r1)}((r1, r2, r3, r4))
    end
    vals = []
    for i in 1:n
        push!(vals, c * v[i])
    end
    return SVector(vals...)
end

Base.:*(v::StaticVector, c::Number) = c * v

# Scalar division (Issue #8125). Upstream defines `A / c` for every static array,
# but the bundled package only had scalar `*`; the missing `/` raised a
# `MethodError` on the generic `operator`. Same special-cased fast paths as `*`.
function Base.:/(v::StaticVector, c::Number)
    n = length(v)
    if n == 2
        vd = v.data
        r1 = vd[1] / c
        r2 = vd[2] / c
        return SVector{2,typeof(r1)}((r1, r2))
    elseif n == 3
        vd = v.data
        r1 = vd[1] / c
        r2 = vd[2] / c
        r3 = vd[3] / c
        return SVector{3,typeof(r1)}((r1, r2, r3))
    elseif n == 4
        vd = v.data
        r1 = vd[1] / c
        r2 = vd[2] / c
        r3 = vd[3] / c
        r4 = vd[4] / c
        return SVector{4,typeof(r1)}((r1, r2, r3, r4))
    end
    vals = []
    for i in 1:n
        push!(vals, v[i] / c)
    end
    return SVector(vals...)
end

# Matrix scalar division. The inline representations (≤4 elements) and the
# square sizes that dominate real workloads (2×2 / 3×3 / 4×4) read the
# column-major `.data` tuple directly and rebuild with a literal-size
# constructor. Other static-matrix shapes need a runtime-parameter
# `SMatrix{M,N}` constructor, which the VM does not yet support, so they are out
# of scope for this fix (tracked in Issue #8125).
function Base.:/(A::StaticMatrix, c::Number)
    s = size(A)
    m = s[1]
    n = s[2]
    d = A.data
    if m == 2 && n == 2
        return SMatrix{2,2}((d[1] / c, d[2] / c, d[3] / c, d[4] / c))
    elseif m == 3 && n == 3
        return SMatrix{3,3}((d[1] / c, d[2] / c, d[3] / c,
                             d[4] / c, d[5] / c, d[6] / c,
                             d[7] / c, d[8] / c, d[9] / c))
    elseif m == 4 && n == 4
        return SMatrix{4,4}((d[1] / c, d[2] / c, d[3] / c, d[4] / c,
                             d[5] / c, d[6] / c, d[7] / c, d[8] / c,
                             d[9] / c, d[10] / c, d[11] / c, d[12] / c,
                             d[13] / c, d[14] / c, d[15] / c, d[16] / c))
    end
    error("StaticMatrix / scalar currently supports square 2×2/3×3/4×4 only (Issue #8125)")
end

# Euclidean norm / normalisation (Issue #8125). The generic `LinearAlgebra`
# versions iterate the array, and iterating a `StaticArrayInline` is unsupported
# by the VM, so they are provided here with index-based accumulation (no
# iteration). Mirrors the approach the bundled Quaternions package uses.
function LinearAlgebra.norm(v::StaticVector)
    n = length(v)
    acc = abs2(v[1])
    for i in 2:n
        acc += abs2(v[i])
    end
    return sqrt(acc)
end

LinearAlgebra.normalize(v::StaticVector) = v / norm(v)

# Unary minus and `map` (Issue #7460, Phase 4). Both return a static result of
# the same shape, mirroring upstream StaticArrays. `_static_bcast_build`
# (broadcast.jl, included earlier) rebuilds the correct static type from a
# column-major value list; the splat constructor handles the vector case.

Base.:-(v::StaticVector) = SVector((map(-, Tuple(v)))...)
Base.:-(A::StaticMatrix) = _static_bcast_build(A, _static_map_values(-, A))

Base.map(f, v::StaticVector) = SVector((map(f, Tuple(v)))...)
Base.map(f, A::StaticMatrix) = _static_bcast_build(A, _static_map_values(f, A))

# Apply `f` to each (column-major) element of a static array, returning a Vector.
function _static_map_values(f, A::StaticArray)
    out = []
    t = Tuple(A)
    for i in 1:length(t)
        push!(out, f(t[i]))
    end
    return out
end

# Conversion (Issue #7460, Phase 4). `convert(SVector{N,T}, x)` coerces each
# element to `T` (the inner tuple constructor stores its argument verbatim, so
# the coercion must happen here) and `convert(SMatrix{M,N,T}, x)` does the same
# while preserving the column-major layout. A same-type convert is the identity.
function Base.convert(::Type{SVector{N,T}}, x::StaticVector) where {N,T}
    vals = []
    t = Tuple(x)
    for i in 1:length(t)
        push!(vals, convert(T, t[i]))
    end
    return SVector{N,T}(vals...)
end

Base.convert(::Type{SVector{N,T}}, x::SVector{N,T}) where {N,T} = x

function Base.convert(::Type{SMatrix{M,N,T}}, x::StaticMatrix) where {M,N,T}
    # An inline loop (not a `y -> convert(T, y)` closure) because a nested lambda
    # cannot capture the method's `T` type parameter in sjulia.
    vals = []
    t = Tuple(x)
    for i in 1:length(t)
        push!(vals, convert(T, t[i]))
    end
    return _static_bcast_build(x, vals)
end

Base.convert(::Type{SMatrix{M,N,T}}, x::SMatrix{M,N,T}) where {M,N,T} = x
