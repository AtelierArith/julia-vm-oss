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
Size(::Type{T}) where {T<:SArray} = StaticSize(_static_shape_tuple(T))
Size(::Type{T}) where {T<:SVector} = StaticSize((T.parameters[1],))
Size(::Type{T}) where {T<:SMatrix} = StaticSize((T.parameters[1], T.parameters[2]))

Length(n::Int64) = StaticLength(n)
Length(x::StaticArray) = Length(typeof(x))
Length(::Type{T}) where {T<:SArray} = StaticLength(_static_length(T))
Length(::Type{T}) where {T<:SVector} = StaticLength(T.parameters[1])
Length(::Type{T}) where {T<:SMatrix} = StaticLength(T.parameters[1] * T.parameters[2])

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
size_to_tuple(::Type{T}) where {T<:SArray} = _static_shape_tuple(T)
size_to_tuple(::Type{T}) where {T<:SVector} = (T.parameters[1],)
size_to_tuple(::Type{T}) where {T<:SMatrix} = (T.parameters[1], T.parameters[2])

function check_array_parameters(size_tuple::Tuple, n::Int64, l::Int64)
    if tuple_length(size_tuple) != n
        error("static array dimension parameter does not match size tuple")
    end
    if tuple_prod(size_tuple) != l
        error("static array length parameter does not match size tuple")
    end
    return true
end

function check_array_parameters(::Type{T}) where {T<:SArray}
    return check_array_parameters(size_to_tuple(T), _static_ndims(T), _static_length(T))
end

convert_ntuple(::Type{T}, xs::Tuple) where {T} = xs

Base.size(x::StaticArray) = size(typeof(x))
Base.size(::Type{T}) where {T<:SArray} = _static_shape_tuple(T)
Base.size(::Type{T}) where {T<:SVector} = (T.parameters[1],)
Base.size(::Type{T}) where {T<:SMatrix} = (T.parameters[1], T.parameters[2])

Base.length(x::StaticArray) = length(typeof(x))
Base.length(::Type{T}) where {T<:SArray} = _static_length(T)
Base.length(::Type{T}) where {T<:SVector} = T.parameters[1]
Base.length(::Type{T}) where {T<:SMatrix} = T.parameters[1] * T.parameters[2]

Base.eltype(x::StaticArray) = eltype(typeof(x))
Base.eltype(::Type{T}) where {T<:SArray} = _static_eltype(T)
Base.eltype(::Type{T}) where {T<:SVector} = T.parameters[2]
Base.eltype(::Type{T}) where {T<:SMatrix} = T.parameters[3]

Base.ndims(x::StaticArray) = ndims(typeof(x))
Base.ndims(::Type{T}) where {T<:SArray} = _static_ndims(T)
Base.ndims(::Type{T}) where {T<:SVector} = 1
Base.ndims(::Type{T}) where {T<:SMatrix} = 2

Base.Tuple(x::SArray) = x.data
Base.Tuple(x::SVector) = x.data
Base.Tuple(x::SMatrix) = x.data

Base.getindex(x::SArray, i::Int64) = x.data[i]
Base.getindex(x::SVector, i::Int64) = x.data[i]

function Base.getindex(x::SMatrix{M,N,T}, i::Int64, j::Int64) where {M,N,T}
    return x.data[(i - 1) * N + j]
end

Base.:(==)(a::StaticSize, b::StaticSize) = a.dims == b.dims
Base.:(==)(a::StaticLength, b::StaticLength) = a.value == b.value

StaticArrayStyle(::Type{T}) where {T<:StaticArray} = StaticArrayStyle{ndims(T)}()
StaticArrayStyle(x::StaticArray) = StaticArrayStyle(typeof(x))
