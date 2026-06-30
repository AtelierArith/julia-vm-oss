abstract type StaticArray{S,T,N} <: AbstractArray{T,N} end
abstract type StaticVecOrMat{S,T,N} <: StaticArray{S,T,N} end
abstract type StaticVector{N,T} <: StaticVecOrMat{Tuple{N},T,1} end
abstract type StaticMatrix{M,N,T} <: StaticVecOrMat{Tuple{M,N},T,2} end
abstract type StaticScalar{T} <: StaticArray{Tuple{},T,0} end

abstract type FieldVector{N,T} <: StaticVector{N,T} end
abstract type FieldMatrix{M,N,T} <: StaticMatrix{M,N,T} end

struct SOneTo
    n::Int64
end

Base.length(r::SOneTo) = r.n
Base.first(r::SOneTo) = 1
Base.last(r::SOneTo) = r.n

struct StaticSize
    dims::Any
end

struct StaticLength
    value::Any
end

struct Dynamic end

struct StaticDimension{N} end

struct StaticArrayStyle{N} end

Size(dims::Tuple) = StaticSize(dims)
Size(dims...) = StaticSize(dims)
Size(x::StaticArray) = Size(typeof(x))

Length(n::Int64) = StaticLength(n)
Length(x::StaticArray) = Length(typeof(x))

similar_type(T) = T
similar_type(::Type{T}, ::Type{S}) where {T<:StaticArray,S} = T
similar_type(::Type{T}, ::Type{S}, size) where {T<:StaticArray,S} = T

tuple_length(t::Tuple) = length(t)

function tuple_prod(t::Tuple)
    p = 1
    for x in t
        p = p * x
    end
    return p
end

function tuple_minimum(t::Tuple)
    length(t) > 0 || error("tuple_minimum requires a non-empty tuple")
    m = t[1]
    for x in t
        if x < m
            m = x
        end
    end
    return m
end

size_to_tuple(s::StaticSize) = s.dims
size_to_tuple(t::Tuple) = t

function check_array_parameters(size_tuple::Tuple, n::Int64, l::Int64)
    if tuple_length(size_tuple) != n
        error("static array dimension parameter does not match size tuple")
    end
    if tuple_prod(size_tuple) != l
        error("static array length parameter does not match size tuple")
    end
    return true
end

convert_ntuple(::Type{T}, xs::Tuple) where {T} = xs

Base.:(==)(a::StaticSize, b::StaticSize) = a.dims == b.dims
Base.:(==)(a::StaticLength, b::StaticLength) = a.value == b.value
