abstract type StaticArray{S,T,N} <: AbstractArray{T,N} end
abstract type StaticVecOrMat{S,T,N} <: StaticArray{S,T,N} end
abstract type StaticVector{N,T} <: StaticVecOrMat{Tuple{N},T,1} end
abstract type StaticMatrix{M,N,T} <: StaticVecOrMat{Tuple{M,N},T,2} end
abstract type StaticScalar{T} <: StaticArray{Tuple{},T,0} end

struct SArray{S,T,N,L} <: StaticArray{S,T,N}
    data::Tuple
end

struct SVector{N,T} <: StaticVector{N,T}
    data::Tuple
end

# `SMatrix{M,N,T,L}` mirrors upstream StaticArraysCore's canonical
# four-parameter alias (`SMatrix{S1,S2,T,L} = SArray{Tuple{S1,S2},T,2,L}`),
# with `L` the flat backing-tuple length, always equal to `M*N` (Issue
# #11542, mirroring the #11432 fix applied to the bundled StaticArrays
# package's own, independent `SMatrix` struct). The bundled package keeps
# `SMatrix` as its own struct (rather than an `SArray` alias, Issue #7458),
# so `L` is validated by `check_array_parameters` (see traits.jl) rather than
# inherited from `SArray`'s inner constructor.
#
# Upstream keeps the 3-parameter (and narrower) spellings constructible via
# incomplete parameterization: `SMatrix{M,N,T}` is `SMatrix{M,N,T,L} where L`
# (a `UnionAll` with `L` free), so a field annotation, `convert` target, or
# constructor call may drop `L` (or `T`, or `N`) and let it be inferred.
# sjulia supports this generically once `L` is a declared struct parameter —
# the `where {M,N,T}` methods below need no `L` themselves.
struct SMatrix{M,N,T,L} <: StaticMatrix{M,N,T}
    data::Tuple
end

abstract type FieldVector{N,T} <: StaticVector{N,T} end
abstract type FieldMatrix{M,N,T} <: StaticMatrix{M,N,T} end

function SVector(xs...)
    return SVector{length(xs), typeof(xs[1])}(xs)
end

function SVector{N,T}(xs...) where {N,T}
    return SVector{N,T}(xs)
end

function SVector{N}(xs...) where {N}
    return SVector{N, typeof(xs[1])}(xs)
end

macro SVector(ex)
    if ex isa Expr && ex.head == :vect
        return Expr(:call, :SVector, ex.args...)
    end
    error("@SVector currently supports literal vector expressions only (Issue #7459)")
end

function SMatrix(xs...)
    n = length(xs)
    return SMatrix{1, n, typeof(xs[1]), n}(xs)
end

function SMatrix{M,N,T,L}(xs...) where {M,N,T,L}
    check_array_parameters((M, N), 2, L)
    # Single-tuple call `SMatrix{M,N,T,L}((a,b,...))` unwraps the flat tuple,
    # stored column-major like upstream StaticArrays (Issue #8084). In
    # practice sjulia's auto-generated default `(data::Tuple)` inner
    # constructor intercepts most single-Tuple calls before this method runs
    # at all (matching real Julia's own method specificity rules) — which is
    # also why `check_array_parameters` above does not validate those calls;
    # see Issue #11573. This unwrap stays as a defensive fallback for the
    # cases that do reach here.
    if length(xs) == 1 && xs[1] isa Tuple
        return SMatrix{M,N,T,L}(xs[1])
    end
    return SMatrix{M,N,T,L}(xs)
end

function SMatrix{M,N,T}(xs...) where {M,N,T}
    # `L` is always inferrable from the flat argument count (Issue #11542,
    # mirroring #11432); `SMatrix{M,N,T,L}`'s own `check_array_parameters`
    # call rejects a length that does not match `M*N`, mirroring upstream's
    # `DimensionMismatch`.
    # Workaround: pass `xs`/`xs[1]` as a single Tuple argument (Issue #11539)
    # rather than re-splatting `xs...` forward — splatting a vararg collection
    # into a runtime type-application curly whose trailing slot is a value
    # expression fails to resolve the expression. (Issue #11539)
    if length(xs) == 1 && xs[1] isa Tuple
        return SMatrix{M,N,T,length(xs[1])}(xs[1])
    end
    return SMatrix{M,N,T,length(xs)}(xs)
end

function SMatrix{M,N}(xs...) where {M,N}
    if length(xs) == 1 && xs[1] isa Tuple
        return SMatrix{M,N, typeof(xs[1][1]), length(xs[1])}(xs[1])
    end
    return SMatrix{M,N, typeof(xs[1]), length(xs)}(xs)
end

macro SMatrix(ex)
    parts = _static_matrix_literal_parts(ex)
    if parts !== nothing
        return Expr(:call, Expr(:curly, :SMatrix, parts[1], parts[2]), parts[3]...)
    end
    error("@SMatrix currently supports literal matrix expressions (Issue #7733)")
end

macro SArray(ex)
    if ex isa Expr && ex.head == :vect
        return Expr(:call, :SVector, ex.args...)
    end
    parts = _static_matrix_literal_parts(ex)
    if parts !== nothing
        return Expr(:call, Expr(:curly, :SMatrix, parts[1], parts[2]), parts[3]...)
    end
    error("@SArray currently supports literal vector and matrix expressions (Issue #7733)")
end

function _static_matrix_literal_parts(ex)
    if ex isa Expr && ex.head == :hcat
        return (1, length(ex.args), ex.args)
    end
    if !(ex isa Expr && ex.head == :vcat)
        return nothing
    end

    rows = ex.args
    m = length(rows)
    if m == 0
        return nothing
    end

    first_row = rows[1]
    n = first_row isa Expr && first_row.head == :row ? length(first_row.args) : 1
    rowmajor = []
    for row in rows
        if row isa Expr && row.head == :row
            if length(row.args) != n
                error("matrix literal rows must have the same length")
            end
            for value in row.args
                push!(rowmajor, value)
            end
        else
            if n != 1
                error("matrix literal rows must have the same length")
            end
            push!(rowmajor, row)
        end
    end
    # Reorder the row-major source elements into column-major storage order to
    # match upstream StaticArrays / Julia (Issue #8084): the SMatrix backing
    # tuple lists column 1 top-to-bottom, then column 2, etc.
    values = []
    for j in 1:n
        for i in 1:m
            push!(values, rowmajor[(i - 1) * n + j])
        end
    end
    return (m, n, values)
end

function _static_shape_tuple(::Type{T}) where {T<:SArray}
    return tuple(T.parameters[1].parameters...)
end

function _static_length(::Type{T}) where {T<:SArray}
    return T.parameters[4]
end

function _static_eltype(::Type{T}) where {T<:SArray}
    return T.parameters[2]
end

function _static_ndims(::Type{T}) where {T<:SArray}
    return T.parameters[3]
end
