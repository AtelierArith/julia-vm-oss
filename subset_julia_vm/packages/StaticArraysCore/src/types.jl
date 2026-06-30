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

struct SMatrix{M,N,T} <: StaticMatrix{M,N,T}
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
    return SMatrix{1, length(xs), typeof(xs[1])}(xs)
end

function SMatrix{M,N,T}(xs...) where {M,N,T}
    return SMatrix{M,N,T}(xs)
end

function SMatrix{M,N}(xs...) where {M,N}
    return SMatrix{M,N, typeof(xs[1])}(xs)
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
