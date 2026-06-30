Size(::Type{T}) where {T<:SArray} = StaticSize(_static_shape_tuple(T))
Size(::Type{T}) where {T<:SVector} = StaticSize((T.parameters[1],))
Size(::Type{T}) where {T<:SMatrix} = StaticSize((T.parameters[1], T.parameters[2]))

Length(::Type{T}) where {T<:SArray} = StaticLength(_static_length(T))
Length(::Type{T}) where {T<:SVector} = StaticLength(T.parameters[1])
Length(::Type{T}) where {T<:SMatrix} = StaticLength(T.parameters[1] * T.parameters[2])

size_to_tuple(::Type{T}) where {T<:SArray} = _static_shape_tuple(T)
size_to_tuple(::Type{T}) where {T<:SVector} = (T.parameters[1],)
size_to_tuple(::Type{T}) where {T<:SMatrix} = (T.parameters[1], T.parameters[2])

function check_array_parameters(::Type{T}) where {T<:SArray}
    return check_array_parameters(size_to_tuple(T), _static_ndims(T), _static_length(T))
end

Base.size(x::StaticArray) = size(typeof(x))
# Fast value paths (Issue #7956): read the dims straight from the concrete
# struct's type parameters via the where-clause, instead of routing through
# `size(typeof(x))` → `T.parameters[i]`. Explicit `.parameters` indexing is ~20x
# more expensive in the VM than where-clause destructuring and dominates the
# static-array arithmetic hot loops (every `*`/`+`/`-` calls `size`/`length`).
Base.size(x::SVector{N,T}) where {N,T} = (N,)
Base.size(x::SMatrix{M,N,T}) where {M,N,T} = (M, N)
Base.size(::Type{T}) where {T<:SArray} = _static_shape_tuple(T)
Base.size(::Type{T}) where {T<:SVector} = (T.parameters[1],)
Base.size(::Type{T}) where {T<:SMatrix} = (T.parameters[1], T.parameters[2])

# Per-dimension size (Issue #7460): `size(A, d)` returns the length of dimension
# `d`, or 1 for `d` beyond `ndims` (upstream convention). Used by broadcast and
# linear-algebra paths that ask for one extent at a time.
function Base.size(x::StaticArray, d::Int64)
    s = size(x)
    return d <= length(s) ? s[d] : 1
end

Base.length(x::StaticArray) = length(typeof(x))
# Fast value paths (Issue #7956), see the size note above.
Base.length(x::SVector{N,T}) where {N,T} = N
Base.length(x::SMatrix{M,N,T}) where {M,N,T} = M * N
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
    # Column-major (upstream StaticArrays / Julia convention, Issue #8084):
    # element (i,j) of an M×N matrix is stored at data[(j-1)*M + i].
    return x.data[(j - 1) * M + i]
end

StaticArrayStyle(::Type{T}) where {T<:StaticArray} = StaticArrayStyle{ndims(T)}()
StaticArrayStyle(x::StaticArray) = StaticArrayStyle(typeof(x))
