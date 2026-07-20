# =============================================================================
# Array{T,N} - Pure Julia mutable struct definition (Issue #2760, #6648)
# =============================================================================
# Based on Julia's base/array.jl (Julia 1.11+)
#
# In official Julia, Array{T,N} wraps a MemoryRef{T} plus size::NTuple{N,Int}.
# This struct mirrors that upstream shape now that MemoryRef{T} fields and
# integer value parameters are supported (Issues #6623/#6625/#6626). Public
# construction/materialization routes now target this wrapper; the retained
# native `Value::ExprArgs` carrier is a VM/cache/host-boundary compatibility
# path only (Issue #6653).

mutable struct Array{T,N} <: DenseArray{T,N}
    ref::MemoryRef{T}
    size::NTuple{N,Int}
end

function wrap end

# =============================================================================
# Array{T,N} wrapper methods
# =============================================================================
# Based on Julia's base/array.jl, where Array stores a MemoryRef plus dims and
# public operations delegate through that wrapper.

function _array_length_from_size(dims)
    n = 1
    for d in dims
        if d < 0
            throw(DimensionMismatch("array dimensions must be non-negative"))
        end
        n = n * d
    end
    return n
end

function _array_wrap_capacity(m)
    return length(m)
end

function _array_wrap_capacity(ref::MemoryRef{T}) where T
    return length(parent(ref)) + 1 - memoryindex(ref)
end

function _array_wrap_check_capacity(capacity, dims)
    len = _array_length_from_size(dims)
    if len > capacity
        throw(DimensionMismatch("array dimensions exceed backing Memory length"))
    end
    return dims
end

function _array_wrap_check(m, dims)
    return _array_wrap_check_capacity(_array_wrap_capacity(m), dims)
end

function _array_construct(::Type{T}, ref::MemoryRef{T}, dims::Tuple) where T
    n = length(dims)
    if n == 0
        return Array{T,0}(ref, dims)
    elseif n == 1
        return Array{T,1}(ref, dims)
    elseif n == 2
        return Array{T,2}(ref, dims)
    elseif n == 3
        return Array{T,3}(ref, dims)
    elseif n == 4
        return Array{T,4}(ref, dims)
    elseif n == 5
        return Array{T,5}(ref, dims)
    elseif n == 6
        return Array{T,6}(ref, dims)
    elseif n == 7
        return Array{T,7}(ref, dims)
    elseif n == 8
        return Array{T,8}(ref, dims)
    elseif n == 9
        return Array{T,9}(ref, dims)
    elseif n == 10
        return Array{T,10}(ref, dims)
    elseif n == 11
        return Array{T,11}(ref, dims)
    elseif n == 12
        return Array{T,12}(ref, dims)
    elseif n == 13
        return Array{T,13}(ref, dims)
    elseif n == 14
        return Array{T,14}(ref, dims)
    elseif n == 15
        return Array{T,15}(ref, dims)
    elseif n == 16
        return Array{T,16}(ref, dims)
    end
    throw(ArgumentError("Array wrapper supports up to 16 dimensions"))
end

function wrap(::Type{Array}, m::Memory{T}, dims::Tuple) where T
    dims = _array_wrap_check(m, dims)
    return _array_construct(T, memoryref(m), dims)
end

# Workaround: current compile-time inference can project `Memory{T}(n)` with a
# runtime `T` as Array while compiling `similar(a, T, dims...)` (Issue #4018).
function wrap(::Type{Array}, m::Array, dims::Tuple)
    dims = _array_wrap_check(m, dims)
    mem = Memory{eltype(m)}(undef, _array_length_from_size(dims))
    for i in 1:length(mem)
        mem[i] = m[i]
    end
    return _array_construct(eltype(m), memoryref(mem), dims)
end

function wrap(::Type{Array}, ref::MemoryRef{T}, dims::Tuple) where T
    dims = _array_wrap_check_capacity(length(parent(ref)) + 1 - memoryindex(ref), dims)
    return _array_construct(T, ref, dims)
end

function wrap(::Type{Array}, m::Memory{T}, l::Integer) where T
    dims = (l,)
    _array_wrap_check(m, dims)
    return _array_construct(T, memoryref(m), dims)
end

function wrap(::Type{Array}, ref::MemoryRef{T}, l::Integer) where T
    dims = (l,)
    _array_wrap_check_capacity(length(parent(ref)) + 1 - memoryindex(ref), dims)
    return _array_construct(T, ref, dims)
end

function wrap(::Type{Array}, m::Memory{T}) where T
    dims = (length(m),)
    return _array_construct(T, memoryref(m), dims)
end

function _array_size_is_offset_encoded(s)
    return length(s) > 0 && isa(s[1], Tuple)
end

function _array_dims(a::Array{T,N}) where {T,N}
    s = a._size
    if _array_size_is_offset_encoded(s)
        return s[1]
    end
    return s
end

function _array_offset(a::Array{T,N}) where {T,N}
    s = a._size
    if _array_size_is_offset_encoded(s)
        return s[2]
    end
    return 1
end

function _array_memory(a::Array{T,N}) where {T,N}
    return a._mem
end

_array_memory_get(m, i::Int64) = m[i]
_array_memory_get(m::MemoryRef, i::Int64) = memoryrefget(m, i)
_array_memory_set!(m, i::Int64, v) = (m[i] = v; v)
_array_memory_set!(m::MemoryRef, i::Int64, v) = (memoryrefset!(m, i, v); v)

function size(a::Array{T,N}) where {T,N}
    return _array_dims(a)
end

function size(a::Array{T,N}, d::Int) where {T,N}
    dims = _array_dims(a)
    if d > length(dims)
        return 1
    end
    return dims[d]
end

function length(a::Array{T,N}) where {T,N}
    return _array_length_from_size(_array_dims(a))
end

# BitArray family (Issue #6663): BitArrays reuse the native Bool Array storage
# but report their own `BitArray{N}` type, so they no longer match the
# rank-parametric `Array{T,N}` size/length methods after the #6648 refactor
# (`BitVector`/`BitMatrix`/`BitArray{N}` are `AbstractArray`, not `Array`). The
# storage-accessor helpers read the shared `_size`/`_mem` fields regardless of
# the type tag, so giving them `BitArray` methods lets both the explicit
# `BitArray`-typed call sites and the `Array{T,N}` method bodies (which are
# `CallResolved` with a runtime bitarray when a broadcast result is statically
# `Array{Bool}`) operate on bitarrays.
function _array_dims(a::BitArray)
    s = a._size
    if _array_size_is_offset_encoded(s)
        return s[1]
    end
    return s
end

function _array_offset(a::BitArray)
    s = a._size
    if _array_size_is_offset_encoded(s)
        return s[2]
    end
    return 1
end

_array_memory(a::BitArray) = a._mem

size(a::BitArray) = _array_dims(a)

function size(a::BitArray, d::Int)
    dims = _array_dims(a)
    if d > length(dims)
        return 1
    end
    return dims[d]
end

length(a::BitArray) = _array_length_from_size(_array_dims(a))

function ndims(a::Array{T,N}) where {T,N}
    return N
end

function _array_reshape_tuple(a::Array{T,N}, dims::Tuple) where {T,N}
    len = _array_length_from_size(dims)
    if len != length(a)
        throw(DimensionMismatch("new dimensions must be consistent with array length"))
    end
    mem = _array_memory(a)
    if _array_offset(a) == 1
        return wrap(Array, mem, dims)
    end
    return wrap(Array, memoryref(mem, _array_offset(a)), dims)
end

function reshape(a::Array{T,N}, dims::Tuple) where {T,N}
    return _array_reshape_tuple(a, dims)
end

function reshape(a::Array{T,N}, dims::Int...) where {T,N}
    return _array_reshape_tuple(a, dims)
end

# reshape of a range materializes it first (`reshape(1:6, 2, 3)`, Issue #5758).
# Ranges are not stored as arrays, so collect to a Vector and reshape that.
function reshape(r::AbstractRange, dims::Tuple)
    return reshape(collect(r), dims)
end

function reshape(r::AbstractRange, dims::Int...)
    return reshape(collect(r), dims)
end

function _array_similar_tuple(a::Array{T,N}, dims::Tuple) where {T,N}
    len = _array_length_from_size(dims)
    mem = similar(_array_memory(a), len)
    return wrap(Array, mem, dims)
end

function _array_similar_typed_tuple(a::Array{T,N}, ::Type{S}, dims::Tuple) where {T,N,S}
    len = _array_length_from_size(dims)
    mem = Memory{S}(undef, len)
    return wrap(Array, mem, dims)
end

function _array_is_bitarray_surface(a::Array{T,N}) where {T,N}
    tname = string(typeof(a))
    return tname == "BitVector" || tname == "BitMatrix" ||
           (length(tname) >= 9 && tname[1:9] == "BitArray{")
end

function _array_is_bitarray_surface(a::Array)
    tname = string(typeof(a))
    return tname == "BitVector" || tname == "BitMatrix" ||
           (length(tname) >= 9 && tname[1:9] == "BitArray{")
end

function _array_is_bitarray_surface(a)
    tname = string(typeof(a))
    return tname == "BitVector" || tname == "BitMatrix" ||
           (length(tname) >= 9 && tname[1:9] == "BitArray{")
end

_array_bitarray_surface_like(a, result::BitArray) = result

function _array_bitarray_surface_like(a::Array{T,N}, result::Array) where {T,N}
    if eltype(result) != Bool
        return result
    end
    if _array_is_bitarray_surface(a)
        return _mark_bitarray(result)
    end
    return result
end

function _array_bitarray_surface_like(a::Array, result::Array)
    if eltype(result) != Bool
        return result
    end
    if _array_is_bitarray_surface(a)
        return _mark_bitarray(result)
    end
    return result
end

function _array_bitarray_surface_like(a, result::Array)
    if eltype(result) != Bool
        return result
    end
    if _array_is_bitarray_surface(a)
        return _mark_bitarray(result)
    end
    return result
end

function similar(a::Array{T,N}) where {T,N}
    return _array_bitarray_surface_like(a, _array_similar_tuple(a, _array_dims(a)))
end

function similar(a::Array{T,N}, ::Type{S}) where {T,N,S}
    return _array_bitarray_surface_like(a, _array_similar_typed_tuple(a, S, _array_dims(a)))
end

function similar(a::Array{T,N}, dims::Tuple) where {T,N}
    return _array_bitarray_surface_like(a, _array_similar_tuple(a, dims))
end

function similar(a::Array{T,N}, ::Type{S}, dims::Tuple) where {T,N,S}
    return _array_bitarray_surface_like(a, _array_similar_typed_tuple(a, S, dims))
end

function similar(a::Array{T,N}, dims::Int...) where {T,N}
    return _array_bitarray_surface_like(a, _array_similar_tuple(a, dims))
end

function similar(a::Array{T,N}, ::Type{S}, dims::Int...) where {T,N,S}
    return _array_bitarray_surface_like(a, _array_similar_typed_tuple(a, S, dims))
end

function _similar_bitarray(a::Array{T,N}, ::Type{S}, dims::Tuple) where {T,N,S}
    return _array_bitarray_surface_like(a, _array_similar_typed_tuple(a, S, dims))
end

function _similar_bitarray(a::Array, ::Type{S}, dims::Tuple) where S
    result = _array_undef_from_dims(S, dims)
    return _array_bitarray_surface_like(a, result)
end

function _similar_bitarray(a, ::Type{S}, dims::Tuple) where S
    result = _array_undef_from_dims(S, dims)
    return _array_bitarray_surface_like(a, result)
end

similar(a::BitVector) = _similar_bitarray(a, Bool, size(a))
similar(a::BitMatrix) = _similar_bitarray(a, Bool, size(a))
similar(a::BitArray) = _similar_bitarray(a, Bool, size(a))
similar(a::BitVector, ::Type{S}) where S = _similar_bitarray(a, S, size(a))
similar(a::BitMatrix, ::Type{S}) where S = _similar_bitarray(a, S, size(a))
similar(a::BitArray, ::Type{S}) where S = _similar_bitarray(a, S, size(a))
similar(a::BitVector, dims::Tuple) = _similar_bitarray(a, Bool, dims)
similar(a::BitMatrix, dims::Tuple) = _similar_bitarray(a, Bool, dims)
similar(a::BitArray, dims::Tuple) = _similar_bitarray(a, Bool, dims)
similar(a::BitVector, ::Type{S}, dims::Tuple) where S = _similar_bitarray(a, S, dims)
similar(a::BitMatrix, ::Type{S}, dims::Tuple) where S = _similar_bitarray(a, S, dims)
similar(a::BitArray, ::Type{S}, dims::Tuple) where S = _similar_bitarray(a, S, dims)
similar(a::BitVector, dims::Int...) = _similar_bitarray(a, Bool, dims)
similar(a::BitMatrix, dims::Int...) = _similar_bitarray(a, Bool, dims)
similar(a::BitArray, dims::Int...) = _similar_bitarray(a, Bool, dims)
similar(a::BitVector, ::Type{S}, dims::Int...) where S = _similar_bitarray(a, S, dims)
similar(a::BitMatrix, ::Type{S}, dims::Int...) where S = _similar_bitarray(a, S, dims)
similar(a::BitArray, ::Type{S}, dims::Int...) where S = _similar_bitarray(a, S, dims)

function similar(::Type{Array{T}}, dims::Tuple) where T
    len = _array_length_from_size(dims)
    mem = Memory{T}(undef, len)
    return wrap(Array, mem, dims)
end

function similar(::Type{Array{Pair}}, dims::Tuple)
    len = _array_length_from_size(dims)
    mem = Memory{Pair}(undef, len)
    return wrap(Array, mem, dims)
end

function similar(::Type{Array{Pair{K,V}}}, dims::Tuple) where {K,V}
    len = _array_length_from_size(dims)
    mem = Memory{Pair{K,V}}(len)
    return wrap(Array, mem, dims)
end

function similar(::Type{Array{T}}, dims::Int...) where T
    len = _array_length_from_size(dims)
    mem = Memory{T}(undef, len)
    return wrap(Array, mem, dims)
end

function similar(::Type{Array{Pair}}, dims::Int...)
    len = _array_length_from_size(dims)
    mem = Memory{Pair}(undef, len)
    return wrap(Array, mem, dims)
end

function similar(::Type{Array{Pair{K,V}}}, dims::Int...) where {K,V}
    len = _array_length_from_size(dims)
    mem = Memory{Pair{K,V}}(len)
    return wrap(Array, mem, dims)
end

function eltype(::Type{Array{T}}) where T
    return T
end

function eltype(::Type{Array})
    return Any
end

function eltype(::Type{Vector{T}}) where T
    return T
end

function eltype(::Type{Vector})
    return Any
end

function eltype(::Type{Matrix{T}}) where T
    return T
end

function Matrix(A::AbstractMatrix)
    rows, cols = size(A)
    M = Matrix{eltype(A)}(undef, rows, cols)
    for j in 1:cols
        for i in 1:rows
            M[i, j] = A[i, j]
        end
    end
    return M
end

function Matrix{T}(A::AbstractMatrix) where T
    rows, cols = size(A)
    M = Matrix{T}(undef, rows, cols)
    for j in 1:cols
        for i in 1:rows
            M[i, j] = convert(T, A[i, j])
        end
    end
    return M
end

function eltype(a::Array)
    return eltype(typeof(a))
end

function _array_check_linear_index(a, i::Int)
    if !(1 <= i <= length(a))
        # Upstream wraps the scalar index in a tuple: A[10] reports
        # BoundsError(A, (10,)) (Issue #11374).
        throw(BoundsError(a, (i,)))
    end
    return nothing
end

function _array_linear_index(a, indices::Tuple)
    dims = _array_dims(a)
    if length(indices) != length(dims)
        throw(BoundsError(a, indices))
    end
    linear = 1
    stride = 1
    for k in 1:length(indices)
        i = indices[k]
        dim = dims[k]
        if !(1 <= i <= dim)
            # Upstream reports the complete index tuple, not the first
            # offending component (Issue #11374).
            throw(BoundsError(a, indices))
        end
        linear = linear + (i - 1) * stride
        stride = stride * dim
    end
    return linear
end

function _array_linear_index(a, i::Int, j::Int)
    return _array_linear_index(a, (i, j))
end

function _array_linear_index_tail(a, i::Int, j::Int, tail)
    dims = _array_dims(a)
    if 2 + length(tail) != length(dims)
        throw(BoundsError(a, i))
    end
    if !(1 <= i <= dims[1] && 1 <= j <= dims[2])
        throw(BoundsError(a, i))
    end
    linear = i + (j - 1) * dims[1]
    stride = dims[1] * dims[2]
    for k in 1:length(tail)
        idx = tail[k]
        dim = dims[k + 2]
        if !(1 <= idx <= dim)
            throw(BoundsError(a, idx))
        end
        linear = linear + (idx - 1) * stride
        stride = stride * dim
    end
    return linear
end

function _array_memory_index(a, i::Int)
    return _array_offset(a) + i - 1
end

function getindex(a::Array{T,N}) where {T,N}
    if ndims(a) == 0
        return a[1]
    end
    throw(ArgumentError("invalid zero-dimensional array index"))
end

function getindex(a::Array{T,N}, i::Int) where {T,N}
    _array_check_linear_index(a, i)
    return _array_memory_get(_array_memory(a), _array_memory_index(a, i))
end

function getindex(a::Array{T,N}, i::Int, j::Int) where {T,N}
    return _array_memory_get(_array_memory(a), _array_memory_index(a, _array_linear_index(a, i, j)))
end

function getindex(a::Array{T,N}, i::Int, j::Int, I::Int...) where {T,N}
    return _array_memory_get(_array_memory(a), _array_memory_index(a, _array_linear_index_tail(a, i, j, I)))
end

function getindex(a::Array{T,N}, i::Int, c::Colon) where {T,N}
    n = size(a, 2)
    result = similar(a, (n,))
    for j in 1:n
        result[j] = a[i, j]
    end
    return result
end

function getindex(a::Array{T,N}, c::Colon, j::Int) where {T,N}
    m = size(a, 1)
    result = similar(a, (m,))
    for i in 1:m
        result[i] = a[i, j]
    end
    return result
end

function getindex(a::Array{T,N}, i::Int, c1::Colon, c2::Colon) where {T,N}
    n = size(a, 2)
    p = size(a, 3)
    result = similar(a, (n, p))
    for k in 1:p
        for j in 1:n
            result[j, k] = a[i, j, k]
        end
    end
    return result
end

function getindex(a::Array{T,N}, c1::Colon, j::Int, c2::Colon) where {T,N}
    m = size(a, 1)
    p = size(a, 3)
    result = similar(a, (m, p))
    for k in 1:p
        for i in 1:m
            result[i, k] = a[i, j, k]
        end
    end
    return result
end

function getindex(a::Array{T,N}, c1::Colon, c2::Colon, k::Int) where {T,N}
    m = size(a, 1)
    n = size(a, 2)
    result = similar(a, (m, n))
    for j in 1:n
        for i in 1:m
            result[i, j] = a[i, j, k]
        end
    end
    return result
end

function getindex(a::Array{T,N}, c1::Colon, c2::Colon, c3::Colon) where {T,N}
    m = size(a, 1)
    n = size(a, 2)
    p = size(a, 3)
    result = similar(a, (m, n, p))
    for k in 1:p
        for j in 1:n
            for i in 1:m
                result[i, j, k] = a[i, j, k]
            end
        end
    end
    return result
end

function getindex(a::Array{T,N}, r::UnitRange) where {T,N}
    l = length(r)
    result = similar(a, (l,))
    if l == 0
        return result
    end
    start = first(r)
    stop = last(r)
    checkbounds(a, start)
    checkbounds(a, stop)
    for k in 1:l
        result[k] = a[start + k - 1]
    end
    return result
end

function getindex(a::Array{T,N}, c::Colon) where {T,N}
    l = length(a)
    result = similar(a, (l,))
    for k in 1:l
        result[k] = a[k]
    end
    return result
end

function getindex(a::Array{T,N}, inds::Array{Int}) where {T,N}
    l = length(inds)
    result = similar(a, (l,))
    for k in 1:l
        result[k] = a[inds[k]]
    end
    return result
end

function getindex(a::Array{T,N}, mask::Array{Bool}) where {T,N}
    if length(mask) != length(a)
        throw(DimensionMismatch("logical index mask must match array length"))
    end
    l = 0
    for k in 1:length(mask)
        if mask[k]
            l = l + 1
        end
    end
    result = similar(a, (l,))
    out = 1
    for k in 1:length(mask)
        if mask[k]
            result[out] = a[k]
            out = out + 1
        end
    end
    return result
end

function ==(A::Array, B::Array)
    if length(A) != length(B)
        return false
    end
    if ndims(A) != ndims(B)
        return false
    end
    for d in 1:ndims(A)
        if size(A, d) != size(B, d)
            return false
        end
    end
    for i in 1:length(A)
        if (A[i] == B[i]) == false
            return false
        end
    end
    return true
end

function setindex!(a::Array{T,N}, v, i::Int) where {T,N}
    _array_check_linear_index(a, i)
    m = _array_memory(a)
    _array_memory_set!(m, _array_memory_index(a, i), convert(T, v))
    return a
end

function setindex!(a::Array{T,N}, v, i::Int, j::Int) where {T,N}
    idx = _array_linear_index(a, i, j)
    m = _array_memory(a)
    _array_memory_set!(m, _array_memory_index(a, idx), convert(T, v))
    return a
end

function setindex!(a::Array{T,N}, v, i::Int, j::Int, I::Int...) where {T,N}
    idx = _array_linear_index_tail(a, i, j, I)
    m = _array_memory(a)
    _array_memory_set!(m, _array_memory_index(a, idx), convert(T, v))
    return a
end

function setindex!(a::Array{T,N}, v) where {T,N}
    if ndims(a) == 0
        m = _array_memory(a)
        _array_memory_set!(m, _array_memory_index(a, 1), convert(T, v))
        return a
    end
    throw(ArgumentError("invalid zero-dimensional array index"))
end

function push!(a::Array{T,N}, item) where {T,N}
    dims = _array_dims(a)
    if length(dims) != 1
        throw(DimensionMismatch("push! expects a one-dimensional Array"))
    end
    old_len = length(a)
    new_len = old_len + 1
    mem = Memory{T}(undef, new_len)
    for i in 1:old_len
        mem[i] = a[i]
    end
    mem[new_len] = convert(T, item)
    a._mem = mem
    a._size = (new_len,)
    return a
end

# =============================================================================
# Array functions - Pure Julia implementations
# =============================================================================

# sum: compute the sum of all elements (preserves element type)
# With dims keyword: sum along specified dimension (dims=1: columns, dims=2: rows)
function sum(arr; dims=0, init=nothing)
    if dims == 0
        # `init` seeds the accumulator: `sum(arr; init=v) == v + sum(arr)` and
        # `sum(empty; init=v) == v` (Issue #5761).
        if init !== nothing
            result = init
            for x in arr
                result = result + x
            end
            return result
        end
        T = eltype(arr)
        if T == Bool || T == Int8 || T == Int16 || T == Int32
            return _sum_int64_vector(arr)
        elseif T == UInt8 || T == UInt16 || T == UInt32 || T == UInt64
            return _sum_uint64_vector(arr)
        elseif T == Float32
            return _sum_float32_vector(arr)
        else
            n = Int(length(arr))
            if n == 0
                if eltype(arr) == Union{}
                    throw(ArgumentError("reducing over an empty collection is not allowed; consider supplying `init` to the reducer"))
                end
                return zero(eltype(arr))
            end
            result = arr[1]
            for i in 2:n
                result = result + arr[i]
            end
            return result
        end
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        if eltype(arr) == Bool
            result = _array_undef_from_dims(Int64, (1, n))
            for j in 1:n
                s = Int64(0)
                for i in 1:m
                    s = s + Int64(arr[i, j])
                end
                result[1, j] = s
            end
            return result
        else
            result = similar(arr, 1, n)
            for j in 1:n
                s = arr[1, j]
                for i in 2:m
                    s = s + arr[i, j]
                end
                result[1, j] = s
            end
        end
        return result
    elseif dims == 2
        if eltype(arr) == Bool
            result = _array_undef_from_dims(Int64, (m, 1))
            for i in 1:m
                s = Int64(0)
                for j in 1:n
                    s = s + Int64(arr[i, j])
                end
                result[i, 1] = s
            end
            return result
        else
            result = similar(arr, m, 1)
            for i in 1:m
                s = arr[i, 1]
                for j in 2:n
                    s = s + arr[i, j]
                end
                result[i, 1] = s
            end
        end
        return result
    else
        error("sum: dims must be 1 or 2 for matrices")
    end
end

function _sum_int64_vector(arr)
    result = Int64(0)
    for i in 1:length(arr)
        result = result + Int64(arr[i])
    end
    return result
end

function _sum_uint64_vector(arr)
    result = UInt64(0)
    for i in 1:length(arr)
        result = result + UInt64(arr[i])
    end
    return result
end

function _sum_float32_vector(arr)
    result = Float32(0)
    for i in 1:length(arr)
        result = result + arr[i]
    end
    return result
end

# VM-native helper for empty generator reductions. The Rust side inspects the
# native Generator callable because `g.f` is not always representable as a
# direct Pure Julia function value.
function _generator_empty_sum_value(g::Generator)
    throw(ArgumentError("reducing over an empty collection is not allowed; consider supplying `init` to the reducer"))
end

# sum(g::Generator): keep the collect-backed non-empty path because nested
# filtered generators currently rely on collect to preserve mapped values. Empty
# generators still need upstream's mapreduce-empty split: identity mapping can
# use the additive identity, while non-identity mapped generators raise
# ArgumentError (Issue 10618). Keep the keyword method for
# `sum(x for x in itr; init=v)` (Issue 7133).
function sum(g::Generator; init=nothing)
    if init !== nothing
        result = init
        for x in g
            result = result + x
        end
        return result
    end
    values = collect(g)
    if length(values) == 0
        return _generator_empty_sum_value(g)
    end
    return sum(values)
end

# prod: compute the product of all elements
# With dims keyword: product along specified dimension (dims=1: columns, dims=2: rows)
function _prod_value_start(arr, i, j)
    T = eltype(arr)
    if T == Int8 || T == Int16 || T == Int32
        return Int64(arr[i, j])
    elseif T == UInt8 || T == UInt16 || T == UInt32 || T == UInt64
        return UInt64(arr[i, j])
    end
    return arr[i, j]
end

function _prod_value_next(arr, i, j)
    T = eltype(arr)
    if T == Int8 || T == Int16 || T == Int32
        return Int64(arr[i, j])
    elseif T == UInt8 || T == UInt16 || T == UInt32 || T == UInt64
        return UInt64(arr[i, j])
    end
    return arr[i, j]
end

function _prod_linear_value(arr, i)
    T = eltype(arr)
    if T == Int8 || T == Int16 || T == Int32
        return Int64(arr[i])
    elseif T == UInt8 || T == UInt16 || T == UInt32 || T == UInt64
        return UInt64(arr[i])
    end
    return arr[i]
end

function _prod_bool_all(arr)
    for i in 1:length(arr)
        if !arr[i]
            return false
        end
    end
    return true
end

function _prod_empty_value(arr)
    T = eltype(arr)
    if T == Bool
        return true
    elseif T == Int8 || T == Int16 || T == Int32 || T == Int64
        return Int64(1)
    elseif T == UInt8 || T == UInt16 || T == UInt32 || T == UInt64
        return UInt64(1)
    elseif T == Float32
        return Float32(1)
    elseif T == Float64
        return Float64(1)
    elseif T == String
        return ""
    end
    return 1
end

function _prod_column_value(arr, j)
    m = size(arr, 1)
    if eltype(arr) == Bool
        for i in 1:m
            if !arr[i, j]
                return false
            end
        end
        return true
    end
    p = _prod_value_start(arr, 1, j)
    for i in 2:m
        p = p * _prod_value_next(arr, i, j)
    end
    return p
end

function _prod_row_value(arr, i)
    n = size(arr, 2)
    if eltype(arr) == Bool
        for j in 1:n
            if !arr[i, j]
                return false
            end
        end
        return true
    end
    p = _prod_value_start(arr, i, 1)
    for j in 2:n
        p = p * _prod_value_next(arr, i, j)
    end
    return p
end

function prod(arr; dims=0, init=nothing)
    if dims == 0
        # `init` seeds the accumulator: `prod(arr; init=v) == v * prod(arr)` and
        # `prod(empty; init=v) == v` (Issue #5761).
        if init !== nothing
            result = init
            for x in arr
                result = result * x
            end
            return result
        end
        n = length(arr)
        if eltype(arr) == Bool
            return _prod_bool_all(arr)
        end
        if n == 0
            return _prod_empty_value(arr)
        end
        result = _prod_linear_value(arr, 1)
        for i in 2:n
            result = result * _prod_linear_value(arr, i)
        end
        return result
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        first_value = _prod_column_value(arr, 1)
        result = _array_undef_from_dims(typeof(first_value), (1, n))
        result[1, 1] = first_value
        for j in 1:n
            if j == 1
                continue
            end
            result[1, j] = _prod_column_value(arr, j)
        end
        return result
    elseif dims == 2
        first_value = _prod_row_value(arr, 1)
        result = _array_undef_from_dims(typeof(first_value), (m, 1))
        result[1, 1] = first_value
        for i in 1:m
            if i == 1
                continue
            end
            result[i, 1] = _prod_row_value(arr, i)
        end
        return result
    else
        error("prod: dims must be 1 or 2 for matrices")
    end
end

function _prod_signed_vector(arr)
    result = Int64(1)
    for i in 1:length(arr)
        result = result * Int64(arr[i])
    end
    return result
end

function _prod_unsigned_vector(arr)
    result = UInt64(1)
    for i in 1:length(arr)
        result = result * UInt64(arr[i])
    end
    return result
end

function _prod_float32_vector(arr)
    result = Float32(1)
    for i in 1:length(arr)
        result = result * arr[i]
    end
    return result
end

function _prod_bool_vector(arr)
    return _prod_bool_all(arr)
end

function _prod_string_vector(arr)
    result = ""
    for i in 1:length(arr)
        result = result * arr[i]
    end
    return result
end

prod(arr::Vector{Int8}) = _prod_signed_vector(arr)
prod(arr::Vector{Int16}) = _prod_signed_vector(arr)
prod(arr::Vector{Int32}) = _prod_signed_vector(arr)
prod(arr::Vector{UInt8}) = _prod_unsigned_vector(arr)
prod(arr::Vector{UInt16}) = _prod_unsigned_vector(arr)
prod(arr::Vector{UInt32}) = _prod_unsigned_vector(arr)
prod(arr::Vector{UInt64}) = _prod_unsigned_vector(arr)
prod(arr::Vector{Float32}) = _prod_float32_vector(arr)
prod(arr::Vector{Bool}) = _prod_bool_vector(arr)
prod(arr::Vector{String}) = _prod_string_vector(arr)

function prod(g::Generator; init=nothing)
    if init !== nothing
        result = init
        for x in g
            result = result * x
        end
        return result
    end
    arr = collect(g)
    n = length(arr)
    if n == 0
        return _prod_empty_value(arr)
    end
    result = _prod_linear_value(arr, 1)
    for i in 2:n
        result = result * _prod_linear_value(arr, i)
    end
    return result
end

# prod(f, arr) - product of f(x) for each element x
# Based on Julia's base/reduce.jl
function prod(f::Function, arr)
    n = length(arr)
    result = f(arr[1])
    for i in 2:n
        result = result * f(arr[i])
    end
    return result
end

# minimum: find the minimum element
# With dims keyword: minimum along specified dimension (dims=1: columns, dims=2: rows)
function minimum(arr; dims=0, init=nothing)
    if dims == 0
        # `init` (Issue #5684): seed the reduction so an empty collection returns
        # `init`; without it, keep the existing first-element behavior.
        if init !== nothing
            result = init
            for x in arr
                if x < result
                    result = x
                end
            end
            return result
        end
        result = arr[1]
        n = length(arr)
        for i in 2:n
            if arr[i] < result
                result = arr[i]
            end
        end
        return result
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        result = similar(arr, 1, n)
        for j in 1:n
            minval = arr[1, j]
            for i in 2:m
                if arr[i, j] < minval
                    minval = arr[i, j]
                end
            end
            result[1, j] = minval
        end
        return result
    elseif dims == 2
        result = similar(arr, m, 1)
        for i in 1:m
            minval = arr[i, 1]
            for j in 2:n
                if arr[i, j] < minval
                    minval = arr[i, j]
                end
            end
            result[i, 1] = minval
        end
        return result
    else
        error("minimum: dims must be 1 or 2 for matrices")
    end
end

# minimum(f, arr) - minimum of f(x) for each element x
# Based on Julia's base/reduce.jl:674
function minimum(f::Function, arr; init=nothing)
    if init !== nothing
        result = init
        for x in arr
            v = f(x)
            if v < result
                result = v
            end
        end
        return result
    end
    return findmin(f, arr)[1]
end

function minimum(g::Generator; init=nothing)
    seen = false
    result = init
    for x in g
        if !seen && init === nothing
            result = x
            seen = true
        elseif x < result
            result = x
            seen = true
        else
            seen = true
        end
    end
    if !seen && init === nothing
        error("minimum: empty collection")
    end
    return result
end

# maximum: find the maximum element
# With dims keyword: maximum along specified dimension (dims=1: columns, dims=2: rows)
function maximum(arr; dims=0, init=nothing)
    if dims == 0
        # `init` (Issue #5684): seed the reduction so an empty collection returns
        # `init`; without it, keep the existing first-element behavior.
        if init !== nothing
            result = init
            for x in arr
                if x > result
                    result = x
                end
            end
            return result
        end
        result = arr[1]
        n = length(arr)
        for i in 2:n
            if arr[i] > result
                result = arr[i]
            end
        end
        return result
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        result = similar(arr, 1, n)
        for j in 1:n
            maxval = arr[1, j]
            for i in 2:m
                if arr[i, j] > maxval
                    maxval = arr[i, j]
                end
            end
            result[1, j] = maxval
        end
        return result
    elseif dims == 2
        result = similar(arr, m, 1)
        for i in 1:m
            maxval = arr[i, 1]
            for j in 2:n
                if arr[i, j] > maxval
                    maxval = arr[i, j]
                end
            end
            result[i, 1] = maxval
        end
        return result
    else
        error("maximum: dims must be 1 or 2 for matrices")
    end
end

# maximum(f, arr) - maximum of f(x) for each element x
# Based on Julia's base/reduce.jl:647
function maximum(f::Function, arr; init=nothing)
    if init !== nothing
        result = init
        for x in arr
            v = f(x)
            if v > result
                result = v
            end
        end
        return result
    end
    return findmax(f, arr)[1]
end

function maximum(g::Generator; init=nothing)
    seen = false
    result = init
    for x in g
        if !seen && init === nothing
            result = x
            seen = true
        elseif x > result
            result = x
            seen = true
        else
            seen = true
        end
    end
    if !seen && init === nothing
        error("maximum: empty collection")
    end
    return result
end

# =============================================================================
# In-place reduction functions
# =============================================================================
# Based on Julia's base/reducedim.jl
#
# These functions reduce A over the singleton dimensions of r,
# writing results into r. The shape of r determines which dimensions
# are reduced:
#   - r is a column vector (m×1): reduce along dim 2 (sum rows)
#   - r is a row vector (1×n): reduce along dim 1 (sum columns)

# sum!: sum elements of A over singleton dimensions of r, write to r
function _sum_value(A, i, j)
    T = eltype(A)
    if T == Bool || T == Int8 || T == Int16 || T == Int32
        return Int64(A[i, j])
    elseif T == UInt8 || T == UInt16 || T == UInt32 || T == UInt64
        return UInt64(A[i, j])
    end
    return A[i, j]
end

function _sum_column_value(A, j)
    m = size(A, 1)
    s = _sum_value(A, 1, j)
    for i in 2:m
        s = s + _sum_value(A, i, j)
    end
    return s
end

function _sum_row_value(A, i)
    n = size(A, 2)
    s = _sum_value(A, i, 1)
    for j in 2:n
        s = s + _sum_value(A, i, j)
    end
    return s
end

function _sum_store_1d!(r, i, value)
    if eltype(r) == Bool && !(value == 0 || value == 1)
        error("InexactError: Bool")
    end
    r[i] = value
    return r
end

function _sum_store_2d!(r, i, j, value)
    if eltype(r) == Bool && !(value == 0 || value == 1)
        error("InexactError: Bool")
    end
    r[i, j] = value
    return r
end

function sum!(r, A)
    sr = size(r)
    sa = size(A)
    ndr = length(sr)
    nda = length(sa)
    if ndr == 1 && nda == 2
        # r is vector of length m → reduce along dim 2
        m = sa[1]
        for i in 1:m
            _sum_store_1d!(r, i, _sum_row_value(A, i))
        end
    elseif ndr == 2 && nda == 2
        m = sa[1]
        n = sa[2]
        if sr[1] == 1 && sr[2] == n
            # r is 1×n → reduce along dim 1
            for j in 1:n
                _sum_store_2d!(r, 1, j, _sum_column_value(A, j))
            end
        elseif sr[1] == m && sr[2] == 1
            # r is m×1 → reduce along dim 2
            for i in 1:m
                _sum_store_2d!(r, i, 1, _sum_row_value(A, i))
            end
        else
            error("sum!: output dimensions must match input along non-reduced dimensions")
        end
    else
        error("sum!: unsupported array dimensions")
    end
    return r
end

# prod!: product of elements of A over singleton dimensions of r, write to r
function prod!(r, A)
    sr = size(r)
    sa = size(A)
    ndr = length(sr)
    nda = length(sa)
    if ndr == 1 && nda == 2
        m = sa[1]
        for i in 1:m
            r[i] = _prod_row_value(A, i)
        end
    elseif ndr == 2 && nda == 2
        m = sa[1]
        n = sa[2]
        if sr[1] == 1 && sr[2] == n
            for j in 1:n
                r[1, j] = _prod_column_value(A, j)
            end
        elseif sr[1] == m && sr[2] == 1
            for i in 1:m
                r[i, 1] = _prod_row_value(A, i)
            end
        else
            error("prod!: output dimensions must match input along non-reduced dimensions")
        end
    else
        error("prod!: unsupported array dimensions")
    end
    return r
end

# maximum!: maximum of A over singleton dimensions of r, write to r
function maximum!(r, A)
    sr = size(r)
    sa = size(A)
    ndr = length(sr)
    nda = length(sa)
    if ndr == 1 && nda == 2
        m = sa[1]
        n = sa[2]
        for i in 1:m
            maxval = A[i, 1]
            for j in 2:n
                if A[i, j] > maxval
                    maxval = A[i, j]
                end
            end
            r[i] = maxval
        end
    elseif ndr == 2 && nda == 2
        m = sa[1]
        n = sa[2]
        if sr[1] == 1 && sr[2] == n
            for j in 1:n
                maxval = A[1, j]
                for i in 2:m
                    if A[i, j] > maxval
                        maxval = A[i, j]
                    end
                end
                r[1, j] = maxval
            end
        elseif sr[1] == m && sr[2] == 1
            for i in 1:m
                maxval = A[i, 1]
                for j in 2:n
                    if A[i, j] > maxval
                        maxval = A[i, j]
                    end
                end
                r[i, 1] = maxval
            end
        else
            error("maximum!: output dimensions must match input along non-reduced dimensions")
        end
    else
        error("maximum!: unsupported array dimensions")
    end
    return r
end

# minimum!: minimum of A over singleton dimensions of r, write to r
function minimum!(r, A)
    sr = size(r)
    sa = size(A)
    ndr = length(sr)
    nda = length(sa)
    if ndr == 1 && nda == 2
        m = sa[1]
        n = sa[2]
        for i in 1:m
            minval = A[i, 1]
            for j in 2:n
                if A[i, j] < minval
                    minval = A[i, j]
                end
            end
            r[i] = minval
        end
    elseif ndr == 2 && nda == 2
        m = sa[1]
        n = sa[2]
        if sr[1] == 1 && sr[2] == n
            for j in 1:n
                minval = A[1, j]
                for i in 2:m
                    if A[i, j] < minval
                        minval = A[i, j]
                    end
                end
                r[1, j] = minval
            end
        elseif sr[1] == m && sr[2] == 1
            for i in 1:m
                minval = A[i, 1]
                for j in 2:n
                    if A[i, j] < minval
                        minval = A[i, j]
                    end
                end
                r[i, 1] = minval
            end
        else
            error("minimum!: output dimensions must match input along non-reduced dimensions")
        end
    else
        error("minimum!: unsupported array dimensions")
    end
    return r
end

# argmin: find index of minimum element
function argmin(arr)
    idx = 1
    val = arr[1]
    n = length(arr)
    for i in 2:n
        if arr[i] < val
            val = arr[i]
            idx = i
        end
    end
    return idx
end

# argmax: find index of maximum element
function argmax(arr)
    idx = 1
    val = arr[1]
    n = length(arr)
    for i in 2:n
        if arr[i] > val
            val = arr[i]
            idx = i
        end
    end
    return idx
end

# collect(arr::Array): type-preserving shallow copy.
# Issue #3648: the generic `collect(itr)` in iterators.jl returns Vector{Any}
# because it has no compile-time element-type information. With this typed
# overload, runtime dispatch (CallDynamic) routes Array values here instead,
# and `similar(arr, n)` allocates a buffer matching arr's element type.
function collect(arr::Array)
    n = length(arr)
    result = similar(arr)
    for i in 1:n
        result[i] = arr[i]
    end
    return result
end

# Vector(::AbstractRange) — materialize a range into a Vector
# (Issue #4810). Mirrors upstream `Base.Vector(r::AbstractRange) = collect(r)`.
function Vector(r::AbstractRange)
    return collect(r)
end

# Vector(::AbstractVector) — copy an existing vector.
# Mirrors upstream `Array{T,N}(x::AbstractArray) = copyto!(Array{T,N}(undef,
# size(x)), x)` for the common no-eltype `Vector(v)` spelling. This method is
# primarily reached through dynamic dispatch (`map(Vector, xs)`); direct
# constructor syntax is intercepted in the compiler and routed through
# `collect(v)` as the same copy operation. Issue #10085.
function Vector(a::AbstractVector)
    return collect(a)
end

# Vector{T}(::AbstractRange) — materialize and convert each element.
# Kept for compatibility with dynamic-dispatch call sites (the compile
# intercept at `compile_array_constructor` short-circuits before
# reaching here). Mirrors upstream `Base.Vector{T}(r::AbstractRange) =
# T[x for x in r]`, implemented with a manual loop to avoid the
# typed-splat (`T[r...]`) path that is still unsupported. See #4811
# for the compile-time intercept that handles the typed case directly.
function Vector{T}(r::AbstractRange) where {T}
    n = length(r)
    result = _array_undef_from_dims(T, (n,))
    for i in 1:n
        result[i] = T(r[i])
    end
    return result
end

# convert(::Type{Vector{S}}, a) — recursive container conversion (Issue #5111).
#
# Mirrors upstream `julia/base/array.jl`:
#   convert(::Type{T}, a::AbstractArray) where {T<:Array} = a isa T ? a : T(a)::T
# and the constructor `Array{T,N}(x::AbstractArray) = copyto!(similar..., x)`,
# which builds a freshly allocated T-element array and converts each element via
# `convert(S, x[i])`. When the source already has the exact target type the
# original object is returned unchanged (identity), matching `a isa T ? a`.
# A non-convertible element raises the same error as `convert(S, e)` (e.g.
# InexactError for `convert(Vector{Int}, [1.5])`).
#
# The element conversion recurses through `convert`, so nested numeric element
# types (`Vector{Float64}` ← `Vector{Int}`) work; nested *parametric container*
# element types (`Vector{Vector{Float64}}`) still depend on the element-type
# carrier, tracked under the type-loss umbrella #5073.
function _convert_to_typed_vector(::Type{S}, a) where {S}
    n = length(a)
    result = _array_undef_from_dims(S, (n,))
    i = 1
    for e in a
        result[i] = convert(S, e)
        i += 1
    end
    return result
end

# Vector{T}(::AbstractVector) — copy and convert each element.
# Constructor spelling must allocate even when the source already has the
# requested element type; `convert(Vector{T}, a)` below keeps Julia's identity
# fast path for exact-type conversion. Issue #10405.
function Vector{T}(a::AbstractVector) where {T}
    return _convert_to_typed_vector(T, a)
end

# Array(::AbstractVector/AbstractRange) and Array{T}(...) are callable type
# values in higher-order dispatch (`map(Array, xs)`, `map(Array{T}, xs)`), not
# just syntax intercepted by the compiler. Mirror the 1-D upstream constructor
# behavior here so those first-class calls allocate fresh vectors. Issue #10405.
function Array(a::AbstractVector)
    return collect(a)
end

function Array(r::AbstractRange)
    return collect(r)
end

function Array{T}(a::AbstractVector) where {T}
    return _convert_to_typed_vector(T, a)
end

function Array{T}(r::AbstractRange) where {T}
    return _convert_to_typed_vector(T, r)
end

function convert(::Type{Vector{S}}, a::AbstractArray) where {S}
    a isa Vector{S} && return a
    return _convert_to_typed_vector(S, a)
end

# Ranges are `<: AbstractArray` upstream, but this VM keeps `AbstractRange`
# outside the `AbstractArray` hierarchy, so the `AbstractArray` method above
# does not catch `convert(Vector{S}, 1:3)`. This explicit method mirrors the
# upstream result (materialize the range, converting each element).
function convert(::Type{Vector{S}}, a::AbstractRange) where {S}
    return _convert_to_typed_vector(S, a)
end

# convert(::Type{Array{S}}, a) — the element-typed `Array` spelling (no rank
# parameter) routes to the same Vector path for 1-D sources, mirroring upstream
# where `Array{T}` and `Vector{T}` share the `T<:Array` convert method.
function convert(::Type{Array{S}}, a::AbstractArray) where {S}
    a isa Vector{S} && return a
    return _convert_to_typed_vector(S, a)
end

function convert(::Type{Array{S}}, a::AbstractRange) where {S}
    return _convert_to_typed_vector(S, a)
end

# Helper for the Vector{Any}(arr) compile-time intercept (Issue #4818).
# Synthesizing `Any[x for x in arr]` from the compile intercept does
# not work because the typed-comprehension lowering wraps the body in
# `Any(x)`, and `Any(...)` is not a defined Julia constructor. Calling
# this helper avoids that — it allocates a `Vector{Any}` (which the
# `Vector{T}(undef, n)` intercept handles correctly) and copies the
# source elements in via plain assignment, which boxes each element to
# `Any` as a side effect of the Memory-backed `Vector{Any}` store.
function _vector_any_collect(arr)
    n = length(arr)
    result = Vector{Any}(undef, n)
    for i in 1:n
        result[i] = arr[i]
    end
    return result
end

# collect(::Type{T}, itr): typed iterator materialization.
# Based on Julia's base/array.jl:
#   collect(::Type{T}, itr) where {T} = _collect(T, itr, IteratorSize(itr))
function collect(::Type{T}, itr) where {T}
    return _collect(T, itr, IteratorSize(itr))
end

# reverse: reverse an array (type-preserving)
function reverse(arr)
    n = length(arr)
    result = collect(arr)  # Create type-preserving copy
    for i in 1:n
        result[i] = arr[n - i + 1]
    end
    return result
end

# reverse(v, start[, stop]): return a copy with only the subrange [start, stop]
# reversed (stop defaults to the last index). Vectors only — `reverse(::String,
# i, j)` is a MethodError upstream (Issue #5693).
function reverse(arr::AbstractVector, start::Integer, stop::Integer)
    result = collect(arr)
    i = start
    j = stop
    while i < j
        tmp = result[i]
        result[i] = result[j]
        result[j] = tmp
        i = i + 1
        j = j - 1
    end
    return result
end

function reverse(arr::AbstractVector, start::Integer)
    return reverse(arr, start, length(arr))
end

# Note: count(f, arr) is implemented as a builtin HOF
# because the VM doesn't yet support calling function parameters

# issorted: check if array is sorted in ascending order
function issorted(arr)
    n = length(arr)
    nm1 = n - 1
    for i in 1:nm1
        if arr[i] > arr[i+1]
            return false
        end
    end
    return true
end

# =============================================================================
# Array manipulation functions
# =============================================================================

# circshift: circular shift array by k positions (type-preserving)
# Positive k shifts right, negative k shifts left
function circshift(arr, k)
    n = length(arr)
    if n == 0
        return collect(arr)  # Return type-preserving empty array
    end
    # Normalize k to be in range [0, n)
    k = mod(k, n)
    if k == 0
        return collect(arr)  # Return type-preserving copy
    end
    result = collect(arr)  # Create type-preserving copy
    for i in 1:n
        # New position after shifting right by k
        new_i = mod(i - 1 + k, n) + 1
        result[new_i] = arr[i]
    end
    return result
end

# circshift!: circular shift array in place
# Based on Julia's base/abstractarray.jl:3655
# Uses the "block swap" algorithm with three reverses
function circshift!(arr, shift)
    n = length(arr)
    if n == 0
        return arr
    end
    shift = mod(shift, n)
    if shift == 0
        return arr
    end
    # Block swap algorithm:
    # 1. Reverse first part [1, n-shift]
    # 2. Reverse second part [n-shift+1, n]
    # 3. Reverse entire array
    reverse!(arr, 1, n - shift)
    reverse!(arr, n - shift + 1, n)
    reverse!(arr)
    return arr
end

function _array_string_copy_same_shape(arr)
    flat = String[]
    len = length(arr)
    for i in 1:len
        push!(flat, arr[i])
    end
    return reshape(flat, size(arr, 1), size(arr, 2))
end

# repeat: repeat array n times.
# Julia-style multiple dispatch: this handles arrays; string repeat is handled
# by a builtin. Type annotation ensures this only matches Array, not String.
#
# Element type is preserved via `similar(arr, total)` (Issue #3587, #3648)
# now that the multi-dim `similar` dispatch (#3751) and the `Any`-dim relax
# (#3777) make this routing reliable from inside a function body. Previously
# this allocated `result = []` (Vector{Any}) because `similar(arr, len * n)`
# fell through to method dispatch.
function repeat(arr::Array, n::Int)
    len = length(arr)
    total = len * n
    result = similar(arr, total)
    k = 1
    for _ in 1:n
        for i in 1:len
            result[k] = arr[i]
            k = k + 1
        end
    end
    return result
end

# repeat(v; inner=k, outer=m): repeat each element `inner` times, then repeat the
# whole result `outer` times (Issue #5699). `repeat([1,2], inner=2)` is
# [1,1,2,2]; `repeat([1,2], outer=2)` is [1,2,1,2]. The positional `repeat(v, n)`
# above is unaffected (different signature).
function repeat(arr::AbstractVector; inner=1, outer=1)
    expanded = similar(arr, 0)
    for x in arr
        for _ in 1:inner
            push!(expanded, x)
        end
    end
    result = similar(arr, 0)
    for _ in 1:outer
        append!(result, expanded)
    end
    return result
end

# Matrix rotation functions: rotl90 / rotr90 / rot180.
#
# Implementation note (Issue #3589, simplified by #3761 / #3751):
#   Element type is preserved by allocating the result via
#   `similar(mat, dims...)` — the multi-dim `similar` builtin (Issue #3751,
#   PR #3757) returns a Matrix{T} matching the input runtime element type.
#   The earlier workaround built a typed flat vector via push! and called
#   reshape; that path is no longer needed now that multi-dim similar is
#   available.

# Internal helpers: each writes column-major into the (already-allocated)
# `result` matrix and returns it.
#
# For an input of size (m, n):
#   rotl90 — output (n, m): out[i, j] = mat[j, n - i + 1]
#   rotr90 — output (n, m): out[i, j] = mat[m - j + 1, i]
#   rot180 — output (m, n): out[i, j] = mat[m - i + 1, n - j + 1]
function _rotl90_into!(result, mat, m, n)
    for j in 1:m
        for i in 1:n
            result[i, j] = mat[j, n - i + 1]
        end
    end
    return result
end

function _rotr90_into!(result, mat, m, n)
    for j in 1:m
        for i in 1:n
            result[i, j] = mat[m - j + 1, i]
        end
    end
    return result
end

function _rot180_into!(result, mat, m, n)
    for j in 1:n
        for i in 1:m
            result[i, j] = mat[m - i + 1, n - j + 1]
        end
    end
    return result
end

# rotl90: rotate matrix 90 degrees counter-clockwise
# For matrix with size (m, n), result has size (n, m).
# Note: inline `similar(mat, size(mat, 2), size(mat, 1))` fails compile-time
# inference (the dim args are seen as `Any`), but the same call with the
# dims hoisted into local variables succeeds — `size` is inferred as I64
# only via the local-variable type table. Hence one body, used by both the
# typed compile-time hints and the generic fallback.
function _rotl90_impl(mat)
    m = size(mat, 1)
    n = size(mat, 2)
    return _rotl90_into!(similar(mat, n, m), mat, m, n)
end

# Typed methods preserved as compile-time hints. Matrix literals now infer as
# `Matrix{T}`; the Vector methods remain for older flat-buffer paths.
rotl90(mat::Matrix{Int64}) = _rotl90_impl(mat)
rotl90(mat::Matrix{Float64}) = _rotl90_impl(mat)
rotl90(mat::Matrix{Bool}) = _rotl90_impl(mat)
rotl90(mat::Matrix{String}) = _rotl90_impl(mat)
rotl90(mat::Matrix{Char}) = _rotl90_impl(mat)
rotl90(mat::Matrix{Any}) = _rotl90_impl(mat)
rotl90(mat::Vector{Int64}) = _rotl90_impl(mat)
rotl90(mat::Vector{Float64}) = _rotl90_impl(mat)
rotl90(mat::Vector{Bool}) = _rotl90_impl(mat)
rotl90(mat::Vector{String}) = _rotl90_impl(mat)
rotl90(mat::Vector{Char}) = _rotl90_impl(mat)
rotl90(mat::Array) = _rotl90_impl(mat)
rotl90(mat) = _rotl90_impl(mat)

# rotr90: rotate matrix 90 degrees clockwise
# For matrix with size (m, n), result has size (n, m).
function _rotr90_impl(mat)
    m = size(mat, 1)
    n = size(mat, 2)
    return _rotr90_into!(similar(mat, n, m), mat, m, n)
end

rotr90(mat::Matrix{Int64}) = _rotr90_impl(mat)
rotr90(mat::Matrix{Float64}) = _rotr90_impl(mat)
rotr90(mat::Matrix{Bool}) = _rotr90_impl(mat)
rotr90(mat::Matrix{String}) = _rotr90_impl(mat)
rotr90(mat::Matrix{Char}) = _rotr90_impl(mat)
rotr90(mat::Matrix{Any}) = _rotr90_impl(mat)
rotr90(mat::Vector{Int64}) = _rotr90_impl(mat)
rotr90(mat::Vector{Float64}) = _rotr90_impl(mat)
rotr90(mat::Vector{Bool}) = _rotr90_impl(mat)
rotr90(mat::Vector{String}) = _rotr90_impl(mat)
rotr90(mat::Vector{Char}) = _rotr90_impl(mat)
rotr90(mat::Array) = _rotr90_impl(mat)
rotr90(mat) = _rotr90_impl(mat)

# rot180: rotate matrix 180 degrees
# For matrix with size (m, n), result has size (m, n).
function _rot180_impl(mat)
    m = size(mat, 1)
    n = size(mat, 2)
    return _rot180_into!(similar(mat, m, n), mat, m, n)
end

rot180(mat::Matrix{Int64}) = _rot180_impl(mat)
rot180(mat::Matrix{Float64}) = _rot180_impl(mat)
rot180(mat::Matrix{Bool}) = _rot180_impl(mat)
rot180(mat::Matrix{String}) = _rot180_impl(mat)
rot180(mat::Matrix{Char}) = _rot180_impl(mat)
rot180(mat::Matrix{Any}) = _rot180_impl(mat)
rot180(mat::Vector{Int64}) = _rot180_impl(mat)
rot180(mat::Vector{Float64}) = _rot180_impl(mat)
rot180(mat::Vector{Bool}) = _rot180_impl(mat)
rot180(mat::Vector{String}) = _rot180_impl(mat)
rot180(mat::Vector{Char}) = _rot180_impl(mat)
rot180(mat::Array) = _rot180_impl(mat)
rot180(mat) = _rot180_impl(mat)

function _copy_array_like(arr)
    n = length(arr)
    result = similar(arr)
    for i in 1:n
        result[i] = arr[i]
    end
    return _array_bitarray_surface_like(arr, result)
end

# copy: create a shallow copy of an array (type-preserving)
copy(arr::BitArray) = _copy_array_like(arr)
copy(arr) = _copy_array_like(arr)

# Note: mean is in Statistics, not Base. Use `using Statistics` to get mean.
# See: subset_julia_vm/src/julia/stdlib/Statistics/src/Statistics.jl

# =============================================================================
# Array concatenation functions
# =============================================================================

# vcat: vertical concatenation of 1D arrays (type-preserving)
# For 1D arrays, concatenates elements sequentially
# Based on Julia's base/abstractarray.jl:1695
function _concat_promoted_eltype(args)
    T = eltype(args[1])
    for i in 2:length(args)
        T = promote_type(T, eltype(args[i]))
    end
    return T
end

# =============================================================================
# General block-aware concatenation (Issue #7203)
# =============================================================================
# Upstream `hcat`/`vcat`/`hvcat` flatten array/range *elements* of a matrix
# literal column-/row-wise into the result. The 1D-vector helpers above only
# treat each argument as a length-N column, which is wrong when an argument is
# itself a 2-D matrix, a range, or a scalar. The helpers below model every
# argument uniformly by its block shape:
#   scalar         -> 1 x 1
#   range / vector -> n x 1   (a column)
#   matrix         -> size(a, 1) x size(a, 2)
# and place blocks side-by-side (hcat) or stacked (vcat), matching Julia's
# `typed_hcat` / `typed_vcat` / `hvcat` semantics.

# Number of rows the argument contributes as a concatenation block.
function _cat_blk_nrows(a)
    if isa(a, AbstractArray)
        if ndims(a) == 1
            return length(a)
        else
            return size(a, 1)
        end
    else
        return 1
    end
end

# Number of columns the argument contributes as a concatenation block.
function _cat_blk_ncols(a)
    if isa(a, AbstractArray)
        if ndims(a) == 1
            return 1
        else
            return size(a, 2)
        end
    else
        return 1
    end
end

# Read element (r, c) of the argument viewed as a block (1-based).
function _cat_blk_get(a, r, c)
    if isa(a, AbstractArray)
        if ndims(a) == 1
            return a[r]
        else
            return a[r, c]
        end
    else
        return a
    end
end

# eltype of an argument (its own type when it is a scalar).
function _cat_arg_eltype(a)
    if isa(a, AbstractArray)
        return eltype(a)
    else
        return typeof(a)
    end
end

# promote_type over the eltypes of all concatenation arguments.
function _cat_promoted_eltype(args)
    T = _cat_arg_eltype(args[1])
    for i in 2:length(args)
        T = promote_type(T, _cat_arg_eltype(args[i]))
    end
    return T
end

# True when every argument is a scalar or a 1-D array/range (no 2-D matrix).
# In that case vertical concatenation produces a 1-D `Vector` (matching
# upstream `vcat` over vectors/scalars) rather than an N x 1 matrix.
function _cat_all_columnish(args)
    for i in 1:length(args)
        a = args[i]
        if isa(a, AbstractArray) && ndims(a) >= 2
            return false
        end
    end
    return true
end

# Horizontal concatenation of scalars / ranges / vectors / matrices.
function _block_hcat(args)
    nargs = length(args)
    nrows = _cat_blk_nrows(args[1])
    ncols = 0
    for i in 1:nargs
        if _cat_blk_nrows(args[i]) != nrows
            error("number of rows of each array must match (got $(_cat_blk_nrows(args[i])) and $(nrows))")
        end
        ncols = ncols + _cat_blk_ncols(args[i])
    end
    result = _array_undef_from_dims(_cat_promoted_eltype(args), (nrows, ncols))
    coloff = 0
    for i in 1:nargs
        a = args[i]
        nc = _cat_blk_ncols(a)
        for c in 1:nc
            for r in 1:nrows
                result[r, coloff + c] = _cat_blk_get(a, r, c)
            end
        end
        coloff = coloff + nc
    end
    return result
end

# Element type of an untyped array literal computed by promoting the types of
# its (already splat-expanded) values. Mirrors upstream `Base.promote_typeof`
# used by `Base.vect(X...)` (Issue #7255).
function _array_literal_promoted_eltype(vals...)
    n = length(vals)
    if n == 0
        return Any
    end
    T = typeof(vals[1])
    for i in 2:n
        T = promote_type(T, typeof(vals[i]))
    end
    return T
end

# Untyped array-literal constructor used when a `[a, xs..., b]` literal contains
# a positional splat. The splat is applied by the caller (the lowering emits a
# splat-call), so by the time we are here each spread element is its own
# argument. Mirrors upstream `Base.vect(X...) = (T = promote_typeof(X...); T[X...])`
# (Issue #7255).
function _array_splat_literal(vals...)
    T = _array_literal_promoted_eltype(vals...)
    return _array_splat_literal_typed(T, vals...)
end

# Typed array-literal constructor used when a `T[a, xs..., b]` literal contains a
# positional splat. The splat is applied by the caller, so each spread element is
# its own argument. Builds a `Vector{T}`, converting each value to `T`, mirroring
# upstream `getindex(::Type{T}, vals...)` (Issue #7255).
function _array_splat_literal_typed(::Type{T}, vals...) where {T}
    n = length(vals)
    v = Vector{T}(undef, n)
    for i in 1:n
        v[i] = convert(T, vals[i])
    end
    return v
end

# Vertical concatenation of scalars / ranges / vectors / matrices.
function _block_vcat(args)
    nargs = length(args)
    ncols = _cat_blk_ncols(args[1])
    nrows = 0
    for i in 1:nargs
        if _cat_blk_ncols(args[i]) != ncols
            # Upstream raises `ArgumentError` for a ragged `hvcat`-style block
            # (e.g. `[1 2; 3 4;; 5 6]`, whose `;;` line-wrap extends only the
            # current row -- Issue #10519) via its own row/column-count
            # validation (`argument count does not match specified shape`,
            # `julia/base/abstractarray.jl` `hvcat`). This block-based
            # concatenation path computes shape differently and cannot
            # reproduce that exact message without a larger rewrite, but the
            # exception TYPE must still match: `error()` raised a generic
            # `ErrorException` here (Issue #10354's fixture-fallout
            # measurement, `array/ncat_double_semicolon_line_wrap_10519.jl`;
            # see docs/vm/EXCEPTION_PARITY.md).
            throw(ArgumentError(
                "number of columns of each array must match (got $(_cat_blk_ncols(args[i])) and $(ncols))",
            ))
        end
        nrows = nrows + _cat_blk_nrows(args[i])
    end
    T = _cat_promoted_eltype(args)
    if ncols == 1 && _cat_all_columnish(args)
        # All column-shaped: produce a 1-D Vector{T}.
        result = _array_undef_from_dims(T, (nrows,))
        k = 1
        for i in 1:nargs
            a = args[i]
            n = _cat_blk_nrows(a)
            for r in 1:n
                result[k] = _cat_blk_get(a, r, 1)
                k = k + 1
            end
        end
        return result
    end
    result = _array_undef_from_dims(T, (nrows, ncols))
    rowoff = 0
    for i in 1:nargs
        a = args[i]
        nr = _cat_blk_nrows(a)
        for c in 1:ncols
            for r in 1:nr
                result[rowoff + r, c] = _cat_blk_get(a, r, c)
            end
        end
        rowoff = rowoff + nr
    end
    return result
end

# True when every argument is a 1-D `Vector` (no scalars, ranges, or matrices).
# These keep the type-preserving fast paths below (Issue #3588).
function _cat_all_plain_vectors(args)
    for i in 1:length(args)
        a = args[i]
        if !(isa(a, AbstractArray) && ndims(a) == 1)
            return false
        end
        if isa(a, AbstractRange)
            return false
        end
    end
    return true
end

function _vcat_typed(args)
    total = 0
    for i in 1:length(args)
        total = total + length(args[i])
    end
    result = _array_undef_from_dims(_concat_promoted_eltype(args), (total,))
    k = 1
    for i in 1:length(args)
        arr = args[i]
        for j in 1:length(arr)
            result[k] = arr[j]
            k = k + 1
        end
    end
    return result
end

function vcat(a, b)
    if _cat_all_plain_vectors((a, b))
        return _vcat_typed((a, b))
    end
    return _block_vcat((a, b))
end

# vcat: varargs version for 3+ arguments
# Based on Julia's base/abstractarray.jl:1966
function vcat(args...)
    n = length(args)
    if n == 0
        return Int64[]
    end
    if n == 1
        return collect(args[1])
    end
    if _cat_all_plain_vectors(args)
        return _vcat_typed(args)
    end
    return _block_vcat(args)
end

# hcat: horizontal concatenation (treats 1D arrays as column vectors)
# Returns a matrix from 1D arrays of same length
# Based on Julia's base/abstractarray.jl:1728
#
# Implementation note (Issue #3588):
#   The previous implementation pre-allocated `result = zeros(...)` which
#   hard-coded the output element type to Float64 and widened any
#   integer/Bool input. We now build a flat typed vector via push! in
#   column-major order and reshape it into a matrix; reshape preserves
#   the element type. To produce a typed Matrix{T} the helper is dispatched
#   on the input element type — concrete `Vector{T}` overloads seed the
#   flat buffer with `T[]`. The generic fallback still produces
#   `Matrix{Any}` because pure-Julia `similar(arr, n, m)` inside a
#   function is blocked by Issue #3648.

# Internal helper: flatten the concatenation column-major into the
# (already-typed) `flat` vector and reshape into an `nrows x ncols` matrix.
# Same shape as the `stack` fix in #3673.
function _hcat_into!(flat, args, nrows)
    ncols = length(args)
    for j in 1:ncols
        arr_j = args[j]
        if length(arr_j) != nrows
            error("hcat: arrays must have same length")
        end
        for i in 1:nrows
            push!(flat, arr_j[i])
        end
    end
    return reshape(flat, nrows, ncols)
end

function _hcat_promoted(args, nrows)
    ncols = length(args)
    result = _array_undef_from_dims(_concat_promoted_eltype(args), (nrows, ncols))
    for j in 1:ncols
        arr_j = args[j]
        if length(arr_j) != nrows
            error("hcat: arrays must have same length")
        end
        for i in 1:nrows
            result[i, j] = arr_j[i]
        end
    end
    return result
end

function _hcat_all_vectors_int64(args)
    for arg in args
        if !isa(arg, Vector{Int64})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_int8(args)
    for arg in args
        if !isa(arg, Vector{Int8})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_int16(args)
    for arg in args
        if !isa(arg, Vector{Int16})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_int32(args)
    for arg in args
        if !isa(arg, Vector{Int32})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_uint8(args)
    for arg in args
        if !isa(arg, Vector{UInt8})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_uint16(args)
    for arg in args
        if !isa(arg, Vector{UInt16})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_uint32(args)
    for arg in args
        if !isa(arg, Vector{UInt32})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_uint64(args)
    for arg in args
        if !isa(arg, Vector{UInt64})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_float64(args)
    for arg in args
        if !isa(arg, Vector{Float64})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_float32(args)
    for arg in args
        if !isa(arg, Vector{Float32})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_int64_or_float64(args)
    for arg in args
        if !(isa(arg, Vector{Int64}) || isa(arg, Vector{Float64}))
            return false
        end
    end
    return true
end

function _hcat_all_vectors_bool(args)
    for arg in args
        if !isa(arg, Vector{Bool})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_string(args)
    for arg in args
        if !isa(arg, Vector{String})
            return false
        end
    end
    return true
end

function _hcat_all_vectors_char(args)
    for arg in args
        if !isa(arg, Vector{Char})
            return false
        end
    end
    return true
end

function _hcat_typed_or_any(args)
    nrows = length(args[1])
    if _hcat_all_vectors_int64(args)
        return _hcat_into!(Int64[], args, nrows)
    elseif _hcat_all_vectors_int8(args)
        return _hcat_into!(Int8[], args, nrows)
    elseif _hcat_all_vectors_int16(args)
        return _hcat_into!(Int16[], args, nrows)
    elseif _hcat_all_vectors_int32(args)
        return _hcat_into!(Int32[], args, nrows)
    elseif _hcat_all_vectors_uint8(args)
        return _hcat_into!(UInt8[], args, nrows)
    elseif _hcat_all_vectors_uint16(args)
        return _hcat_into!(UInt16[], args, nrows)
    elseif _hcat_all_vectors_uint32(args)
        return _hcat_into!(UInt32[], args, nrows)
    elseif _hcat_all_vectors_uint64(args)
        return _hcat_into!(UInt64[], args, nrows)
    elseif _hcat_all_vectors_float64(args)
        return _hcat_into!(Float64[], args, nrows)
    elseif _hcat_all_vectors_float32(args)
        return _hcat_into!(Float32[], args, nrows)
    elseif _hcat_all_vectors_int64_or_float64(args)
        return _hcat_into!(Float64[], args, nrows)
    elseif _hcat_all_vectors_bool(args)
        return _hcat_into!(Bool[], args, nrows)
    elseif _hcat_all_vectors_string(args)
        return _hcat_into!(String[], args, nrows)
    elseif _hcat_all_vectors_char(args)
        return _hcat_into!(Char[], args, nrows)
    else
        return _hcat_promoted(args, nrows)
    end
end

# Type-preserving 2-argument specializations (Issue #3588).
function hcat(a::Vector{Int64}, b::Vector{Int64})
    return _hcat_into!(Int64[], (a, b), length(a))
end
function hcat(a::Vector{Float64}, b::Vector{Float64})
    return _hcat_into!(Float64[], (a, b), length(a))
end
function hcat(a::Vector{Bool}, b::Vector{Bool})
    return _hcat_into!(Bool[], (a, b), length(a))
end
function hcat(a::Vector{String}, b::Vector{String})
    return _hcat_into!(String[], (a, b), length(a))
end
function hcat(a::Vector{Char}, b::Vector{Char})
    return _hcat_into!(Char[], (a, b), length(a))
end

# Generic 2-argument fallback. Mixed element types follow upstream's
# promote_eltype-shaped allocation path. Plain 1-D vectors keep the
# type-preserving fast path; scalars / ranges / matrices flatten block-wise
# (Issue #7203).
function hcat(a, b)
    if _cat_all_plain_vectors((a, b))
        na = length(a)
        nb = length(b)
        if na != nb
            error("hcat: arrays must have same length")
        end
        return _hcat_typed_or_any((a, b))
    end
    return _block_hcat((a, b))
end

# Type-preserving 3-argument specializations (Issue #3588).
function hcat(a::Vector{Int64}, b::Vector{Int64}, c::Vector{Int64})
    return _hcat_into!(Int64[], (a, b, c), length(a))
end
function hcat(a::Vector{Float64}, b::Vector{Float64}, c::Vector{Float64})
    return _hcat_into!(Float64[], (a, b, c), length(a))
end
function hcat(a::Vector{Bool}, b::Vector{Bool}, c::Vector{Bool})
    return _hcat_into!(Bool[], (a, b, c), length(a))
end

# hcat: varargs fallback (1-argument or 4+ arguments, or mixed types).
# Result allocation follows the promoted input vector eltype.
# Based on Julia's base/abstractarray.jl:2016
function hcat(args...)
    n = length(args)
    if n == 0
        error("hcat requires at least one argument")
    end
    if _cat_all_plain_vectors(args)
        return _hcat_typed_or_any(args)
    end
    return _block_hcat(args)
end

# hvcat: build a matrix from row-major blocks. `rows` is a tuple giving the
# number of arguments in each row; arguments are concatenated horizontally
# within a row, then the rows are concatenated vertically (Issue #7203).
# Based on Julia's base/abstractarray.jl:2142
function hvcat(rows::Tuple, args...)
    nbr = length(rows)
    rowblocks = []
    k = 1
    for i in 1:nbr
        nc = rows[i]
        rowargs = []
        for j in 1:nc
            push!(rowargs, args[k])
            k = k + 1
        end
        push!(rowblocks, _block_hcat(rowargs))
    end
    if nbr == 1
        return rowblocks[1]
    end
    return _block_vcat(rowblocks)
end

# Single-Int `rows` form: hvcat(ncols, args...) — one row block size repeated
# until the arguments are exhausted (e.g. hvcat(2, 1, 2, 3, 4) == [1 2; 3 4]).
function hvcat(ncols::Int, args...)
    n = length(args)
    nrows = div(n, ncols)
    rowblocks = []
    k = 1
    for i in 1:nrows
        rowargs = []
        for j in 1:ncols
            push!(rowargs, args[k])
            k = k + 1
        end
        push!(rowblocks, _block_hcat(rowargs))
    end
    if nrows == 1
        return rowblocks[1]
    end
    return _block_vcat(rowblocks)
end

# =============================================================================
# hvncat: N-dimensional block concatenation (Issue #10381)
#
# Mirrors upstream julia/base/abstractarray.jl `hvncat`. Three call forms:
#   hvncat(dim::Int, xs...)                    — concatenate along `dim`
#   hvncat(dims::NTuple{N,Int}, row_first, xs...)   — balanced literal form
#   hvncat(shape::NTuple{N,Tuple}, row_first, xs...) — ragged literal form
# The literal lowering emits the shape form for `;;`/`;;;`-separated
# array-valued blocks; the other forms cover direct Base.hvncat callers.
# Ports of the upstream algorithms with sjulia adaptations: no `Val`
# dispatch, scratch vectors allocated locally, allocation through the shared
# `_cat_promoted_eltype` / `_array_undef_from_dims` helpers.
# =============================================================================

_hvncat_size(x, d) = isa(x, AbstractArray) ? size(x, d) : 1
_hvncat_ndims(x) = isa(x, AbstractArray) ? ndims(x) : 0
_hvncat_length(x) = isa(x, AbstractArray) ? length(x) : 1

function hvncat(dimsshape::Tuple, row_first::Bool, xs...)
    length(dimsshape) > 0 ||
        throw(ArgumentError("`dimsshape` argument must be non-empty"))
    if isa(dimsshape[1], Tuple)
        return _hvncat_shape(dimsshape, row_first, xs)
    end
    return _hvncat_dims(dimsshape, row_first, xs)
end

hvncat(dim::Int, xs...) = _hvncat_along_dim(dim, xs)

# hvncat(dim, xs...): concatenate along a single dimension (upstream
# `_typed_hvncat(T, Val(N), as...)`).
function _hvncat_along_dim(N::Int, as)
    length(as) > 0 ||
        throw(ArgumentError("must have at least one element"))
    N > 0 ||
        throw(ArgumentError("concatenation dimension must be positive"))
    nd = N
    ndim_total = 0
    for i in 1:length(as)
        ndim_total += _hvncat_size(as[i], N)
        nd = max(nd, _hvncat_ndims(as[i]))
    end
    for i in 1:length(as)
        for d in 1:nd
            d == N && continue
            if _hvncat_size(as[1], d) != _hvncat_size(as[i], d)
                throw(DimensionMismatch("mismatched size along axis $d in element $i"))
            end
        end
    end
    outdims = zeros(Int, nd)
    for d in 1:nd
        outdims[d] = (d == N) ? ndim_total : _hvncat_size(as[1], d)
    end
    A = _array_undef_from_dims(_cat_promoted_eltype(as), (outdims...,))
    k = 1
    for a in as
        if isa(a, AbstractArray)
            for x in a
                A[k] = x
                k += 1
            end
        else
            A[k] = a
            k += 1
        end
    end
    return A
end

# Balanced form (upstream `_typed_hvncat_dims`): `dims[d]` is the number of
# blocks along dimension `d`.
function _hvncat_dims(dims::Tuple, row_first::Bool, as)
    length(as) > 0 ||
        throw(ArgumentError("must have at least one element"))
    for d in dims
        d > 0 || throw(ArgumentError("`dims` argument must contain positive integers"))
    end
    nd = length(dims)
    for a in as
        nd = max(nd, _hvncat_ndims(a))
    end
    # pad dims with trailing 1-blocks up to the element rank
    dimsv = ones(Int, nd)
    for d in 1:length(dims)
        dimsv[d] = dims[d]
    end

    d1 = row_first ? 2 : 1
    d2 = row_first ? 1 : 2

    outdims = zeros(Int, nd)

    # discover number of rows or columns
    for i in 1:dimsv[d1]
        outdims[d1] += _hvncat_size(as[i], d1)
    end

    currentdims = zeros(Int, nd)
    blockcount = 0
    elementcount = 0
    for i in 1:length(as)
        elementcount += _hvncat_length(as[i])
        currentdims[d1] += _hvncat_size(as[i], d1)
        if currentdims[d1] == outdims[d1]
            currentdims[d1] = 0
            d = d2
            while d <= nd
                currentdims[d] += _hvncat_size(as[i], d)
                if outdims[d] == 0 # unfixed dimension
                    blockcount += 1
                    if blockcount == dimsv[d]
                        outdims[d] = currentdims[d]
                        currentdims[d] = 0
                        blockcount = 0
                    else
                        break
                    end
                else # fixed dimension
                    if currentdims[d] == outdims[d] # end of dimension
                        currentdims[d] = 0
                    elseif currentdims[d] < outdims[d] # dimension in progress
                        break
                    else # exceeded dimension
                        throw(DimensionMismatch("argument $i has too many elements along axis $d"))
                    end
                end
                d = (d == d2) ? 3 : d + 1
                d == d1 && (d += 1)
            end
        elseif currentdims[d1] > outdims[d1] # exceeded dimension
            throw(DimensionMismatch("argument $i has too many elements along axis $d1"))
        end
    end

    outlen = prod(outdims)
    elementcount == outlen ||
        throw(DimensionMismatch("mismatched number of elements; expected $(outlen), got $(elementcount)"))

    A = _array_undef_from_dims(_cat_promoted_eltype(as), (outdims...,))
    _hvncat_fill!(A, zeros(Int, nd), zeros(Int, nd), d1, d2, as)
    return A
end

# Ragged form (upstream `_typed_hvncat_shape`): `shape[d]` lists the element
# count of each dimension-`d` block, cumulatively per level.
function _hvncat_shape(shape::Tuple, row_first::Bool, as)
    length(as) > 0 ||
        throw(ArgumentError("must have at least one element"))
    N = length(shape)
    nd = N
    for a in as
        nd = max(nd, _hvncat_ndims(a))
    end

    for lvl in shape
        length(lvl) > 0 ||
            throw(ArgumentError("each level of `shape` argument must have at least one value"))
        for v in lvl
            v > 0 || throw(ArgumentError("`shape` argument must consist of positive integers"))
        end
    end
    length(shape[N]) == 1 ||
        throw(ArgumentError("last level of shape must contain only one integer"))
    shape[N][1] == length(as) ||
        throw(ArgumentError("number of elements does not match shape; expected $(shape[N][1]), got $(length(as))"))

    d1 = row_first ? 2 : 1
    d2 = row_first ? 1 : 2

    outdims = fill(-1, nd)
    currentdims = zeros(Int, nd)
    blockcounts = zeros(Int, nd)
    shapepos = ones(Int, nd)

    elementcount = 0
    for i in 1:length(as)
        elementcount += _hvncat_length(as[i])
        wasstartblock = false
        for d in 1:N
            ad = (d < 3 && row_first) ? (d == 1 ? 2 : 1) : d
            dsize = _hvncat_size(as[i], ad)
            blockcounts[d] += 1

            if d == 1 || i == 1 || wasstartblock
                currentdims[d] += dsize
            elseif dsize != _hvncat_size(as[i - 1], ad)
                throw(DimensionMismatch("argument $i has a mismatched number of elements along axis $ad; expected $(_hvncat_size(as[i - 1], ad)), got $dsize"))
            end

            wasstartblock = blockcounts[d] == 1 # remember for next dimension

            isendblock = blockcounts[d] == shape[d][shapepos[d]]
            if isendblock
                if outdims[d] == -1
                    outdims[d] = currentdims[d]
                elseif outdims[d] != currentdims[d]
                    throw(DimensionMismatch("argument $i has a mismatched number of elements along axis $ad; expected $(abs(outdims[d] - (currentdims[d] - dsize))), got $dsize"))
                end
                currentdims[d] = 0
                blockcounts[d] = 0
                shapepos[d] += 1
                if d > 1 && blockcounts[d - 1] != 0
                    throw(DimensionMismatch("shape in level $d is inconsistent; level counts must nest evenly into each other"))
                end
            end
        end
    end

    outlen = prod(outdims)
    elementcount == outlen ||
        throw(ArgumentError("mismatched number of elements; expected $(outlen), got $(elementcount)"))

    if row_first
        tmp = outdims[1]
        outdims[1] = outdims[2]
        outdims[2] = tmp
    end

    A = _array_undef_from_dims(_cat_promoted_eltype(as), (outdims...,))
    _hvncat_fill!(A, currentdims, blockcounts, d1, d2, as)
    return A
end

# Copy the elements into the destination (upstream `hvncat_fill!`): the
# offsets advance along d1, then d2, then dims 3..N.
function _hvncat_fill!(A, offsets, inneroffsets, d1::Int, d2::Int, as)
    nd = ndims(A)
    outdims = size(A)
    # iteration order over destination dimensions when advancing offsets
    order = zeros(Int, nd)
    order[1] = d1
    if nd >= 2
        order[2] = d2
    end
    for j in 3:nd
        order[j] = j
    end
    for a in as
        if isa(a, AbstractArray)
            for x in a
                Ai = _hvncat_calcindex(offsets, inneroffsets, outdims, nd)
                A[Ai] = x
                for j in 1:nd
                    inneroffsets[j] += 1
                    inneroffsets[j] < _hvncat_size(a, j) && break
                    inneroffsets[j] = 0
                end
            end
        else
            Ai = _hvncat_calcindex(offsets, inneroffsets, outdims, nd)
            A[Ai] = a
        end

        for jj in 1:nd
            j = order[jj]
            offsets[j] += _hvncat_size(a, j)
            offsets[j] < outdims[j] && break
            offsets[j] = 0
        end
    end
    return A
end

function _hvncat_calcindex(offsets, inneroffsets, outdims, nd::Int)
    Ai = inneroffsets[1] + offsets[1] + 1
    for j in 2:nd
        increment = inneroffsets[j] + offsets[j]
        for k in 1:(j - 1)
            increment *= outdims[k]
        end
        Ai += increment
    end
    return Ai
end

# vec: flatten array to 1D vector (type-preserving)
function vec(arr)
    return collect(arr)
end

# =============================================================================
# stack: combine arrays into a higher-dimensional array
# =============================================================================
# Based on Julia's Base.stack (Julia 1.9+)

# stack(arrays): stack 1D arrays as columns of a matrix
# Each element of arrays should be a 1D array of the same length.
# Returns a matrix where column j is arrays[j].
function stack(arrays)
    n = length(arrays)
    if n == 0
        return zeros(0, 0)
    end
    first_arr = arrays[1]
    m = length(first_arr)
    # Validate uniform slice lengths up-front so ragged input raises a clear
    # dimension-mismatch error rather than the previous internal index-out-of-
    # bounds (Issue #3592).
    for j in 2:n
        if length(arrays[j]) != m
            throw(DimensionMismatch("stack expects uniform slices, got length(arrays[$j]) == $(length(arrays[j])) while first had $m"))
        end
    end
    first_eltype = eltype(first_arr)
    same_eltype = true
    result_eltype = first_eltype
    for j in 2:n
        if eltype(arrays[j]) != first_eltype
            same_eltype = false
        end
        result_eltype = promote_type(result_eltype, eltype(arrays[j]))
    end
    if same_eltype
        result = similar(first_arr, m, n)
    else
        result = _array_undef_from_dims(result_eltype, (m, n))
    end

    for j in 1:n
        arr = arrays[j]
        for i in 1:m
            result[i, j] = arr[i]
        end
    end
    return result
end

# =============================================================================
# selectdim and dropdims
# =============================================================================
# View-based implementations that share parent storage and preserve element
# type live in subarray.jl alongside MatrixView (Issues #3593, #3594). The
# previous copy-based fallbacks here were removed because they widened the
# element type (allocated via zeros(...)) and broke mutation aliasing.

# insertdims(A; dims): insert singleton dimension at specified position
# Inverse of dropdims. Based on Julia's base/abstractarraymath.jl (Julia 1.12).
# For a 1D vector: dims=1 -> 1×n row, dims=2 -> n×1 column
# For a 2D matrix: dims=3 -> m×n×1 array
function insertdims(A; dims)
    # deepcopy A first because reshape modifies shape in-place in SubsetJuliaVM
    B = deepcopy(A)
    nd = ndims(B)
    if nd == 1
        n = length(B)
        if dims == 1
            return reshape(B, 1, n)
        elseif dims == 2
            return reshape(B, n, 1)
        else
            error("insertdims: dims must be between 1 and $(nd + 1) for $(nd)D arrays")
        end
    elseif nd == 2
        m = size(B, 1)
        n = size(B, 2)
        if dims == 1
            return reshape(B, 1, m, n)
        elseif dims == 2
            return reshape(B, m, 1, n)
        elseif dims == 3
            return reshape(B, m, n, 1)
        else
            error("insertdims: dims must be between 1 and $(nd + 1) for $(nd)D arrays")
        end
    else
        error("insertdims: only 1D and 2D arrays are supported")
    end
end

# =============================================================================
# Array search functions
# =============================================================================

# findfirst: find first index where predicate is satisfied
# Returns the index of first occurrence, or nothing if not found
# Note: String search is handled by builtin findfirst(pattern::String, s::String)
function findfirst(f::Function, arr::Array)
    n = length(arr)
    for i in 1:n
        if f(arr[i])
            return i
        end
    end
    return nothing
end

# findfirst: find first index where value appears in array
function findfirst(value, arr::Array)
    n = length(arr)
    for i in 1:n
        if arr[i] == value
            return i
        end
    end
    return nothing
end

# findlast: find last index where predicate is satisfied
# Returns the index of last occurrence, or nothing if not found
# Note: String search is handled by builtin findlast(pattern::String, s::String)
function findlast(f::Function, arr::Array)
    n = length(arr)
    for i in n:-1:1
        if f(arr[i])
            return i
        end
    end
    return nothing
end

# findlast: find last index where value appears in array
function findlast(value, arr::Array)
    n = length(arr)
    for i in n:-1:1
        if arr[i] == value
            return i
        end
    end
    return nothing
end

# Note: findall(f, arr) is implemented as a builtin higher-order function
# Returns a Vector{Int64} of 1-based indices where predicate f returns true
# See: src/compile/expr/builtin_hof.rs

# findall(A): Single-argument form for boolean/truthy arrays
# Based on Julia's base/array.jl:2812
# Returns Vector{Int64} of indices where A[i] is truthy
function findall(A::Array)
    result = Int64[]
    n = length(A)
    for i in 1:n
        # Direct truthiness check works for Bool values
        if A[i]
            push!(result, i)
        end
    end
    return result
end

# findall(x::Bool): Scalar boolean - returns [1] if true, empty array if false
# Based on Julia's base/array.jl:2825
function findall(x::Bool)
    if x
        return Int64[1]
    else
        return Int64[]
    end
end

# =============================================================================
# Array manipulation functions (additional)
# =============================================================================

# fill!: fill array with a value (mutating)
function fill!(arr, value)
    n = length(arr)
    for i in 1:n
        arr[i] = value
    end
    return arr
end

# copyto!: copy elements from src to dest (mutating)
# Overlap-safe: iterates backward when dest === src && copying forward would
# overwrite an element before it's read (Issue #3595).
function copyto!(dest, src)
    n = length(src)
    if dest === src
        return dest
    end
    for i in 1:n
        dest[i] = src[i]
    end
    return dest
end

# copyto!(dest, dstart, src): copy all of src to dest starting at dest[dstart]
# Based on Julia's base/abstractarray.jl:1126
# Overlap-safe (Issue #3595): when dest === src and dstart > 1, iterate backward.
function copyto!(dest::Array, dstart::Int, src::Array)
    n = length(src)
    if dest === src && dstart > 1
        for i in n:-1:1
            dest[dstart + i - 1] = src[i]
        end
    else
        for i in 1:n
            dest[dstart + i - 1] = src[i]
        end
    end
    return dest
end

# copyto!(dest, dstart, src, sstart): copy from src[sstart:end] to dest[dstart:end]
# Based on Julia's base/abstractarray.jl:1130
# Overlap-safe (Issue #3595): when dest === src and dstart > sstart, iterate backward.
function copyto!(dest::Array, dstart::Int, src::Array, sstart::Int)
    n = length(src) - sstart + 1
    if dest === src && dstart > sstart
        for i in n:-1:1
            dest[dstart + i - 1] = src[sstart + i - 1]
        end
    else
        for i in 1:n
            dest[dstart + i - 1] = src[sstart + i - 1]
        end
    end
    return dest
end

# copyto!(dest, dstart, src, sstart, n): copy n elements from src[sstart] to dest[dstart]
# Based on Julia's base/abstractarray.jl:1136
# Overlap-safe (Issue #3595): when dest === src and dstart > sstart, iterate backward
# (forward iteration would overwrite source elements before they're read).
function copyto!(dest::Array, dstart::Int, src::Array, sstart::Int, n::Int)
    if n == 0
        return dest
    end
    if n < 0
        throw(ArgumentError("Number of elements to copy must be non-negative."))
    end
    if dest === src && dstart > sstart
        for i in n:-1:1
            dest[dstart + i - 1] = src[sstart + i - 1]
        end
    else
        for i in 1:n
            dest[dstart + i - 1] = src[sstart + i - 1]
        end
    end
    return dest
end

# copy!: copy elements from src to dest (mutating), resizing dest if needed
# Based on Julia's base/abstractarray.jl:924
# For vectors/1D arrays: resizes dest to match src length, then copies
function copy!(dest::Array, src::Array)
    if length(dest) != length(src)
        resize!(dest, length(src))
    end
    copyto!(dest, src)
end

# =============================================================================
# Array dimension and iteration functions
# =============================================================================

# ndims: return the number of dimensions of an array
# Based on Julia's base/abstractarray.jl
#   ndims(::AbstractArray{T,N}) where {T,N} = N::Int
# Note: In Julia, ndims extracts N from the type parameter.
# Here we compute it from the size tuple.
function ndims(arr::AbstractArray)
    return length(size(arr))
end

# Type-level ndims (Issue #5118).  Upstream
# (julia/base/abstractarray.jl:278-279) defines
#   ndims(::AbstractArray{T,N}) where {T,N} = N::Int
#   ndims(::Type{<:AbstractArray{<:Any,N}}) where {N} = N::Int
# so `ndims(T)` reads the dimension value parameter `N` directly from the type.
# In sjulia the dimension `N` is a *value* parameter, and binding a value
# parameter through `::Type{Array{T,N}} where {T,N}` is not yet supported
# (the generic `Vararg{T,N}` / `NTuple{N,T}` value-parameter machinery is
# tracked by Issue #5062). Until that lands, the type form is resolved by the
# `Ndims` builtin, which reads the rank directly from the array `DataType`
# (`Vector{T}` -> 1, `Matrix{T}` -> 2, `Array{T,N}` -> N) for any `N`.

# axes: return tuple of index ranges for each dimension
# Supports up to 16 dimensions (covers virtually all practical use cases)
function axes(arr)
    s = size(arr)
    n = length(s)
    if n == 0
        return ()
    elseif n == 1
        return (OneTo(s[1]),)
    elseif n == 2
        return (OneTo(s[1]), OneTo(s[2]))
    elseif n == 3
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]))
    elseif n == 4
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]))
    elseif n == 5
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]))
    elseif n == 6
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]))
    elseif n == 7
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]))
    elseif n == 8
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]))
    elseif n == 9
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]), OneTo(s[9]))
    elseif n == 10
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]), OneTo(s[9]), OneTo(s[10]))
    elseif n == 11
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]), OneTo(s[9]), OneTo(s[10]), OneTo(s[11]))
    elseif n == 12
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]), OneTo(s[9]), OneTo(s[10]), OneTo(s[11]), OneTo(s[12]))
    elseif n == 13
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]), OneTo(s[9]), OneTo(s[10]), OneTo(s[11]), OneTo(s[12]), OneTo(s[13]))
    elseif n == 14
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]), OneTo(s[9]), OneTo(s[10]), OneTo(s[11]), OneTo(s[12]), OneTo(s[13]), OneTo(s[14]))
    elseif n == 15
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]), OneTo(s[9]), OneTo(s[10]), OneTo(s[11]), OneTo(s[12]), OneTo(s[13]), OneTo(s[14]), OneTo(s[15]))
    elseif n == 16
        return (OneTo(s[1]), OneTo(s[2]), OneTo(s[3]), OneTo(s[4]), OneTo(s[5]), OneTo(s[6]), OneTo(s[7]), OneTo(s[8]), OneTo(s[9]), OneTo(s[10]), OneTo(s[11]), OneTo(s[12]), OneTo(s[13]), OneTo(s[14]), OneTo(s[15]), OneTo(s[16]))
    else
        # For dimensions > 16, throw an error
        error("axes: arrays with more than 16 dimensions are not supported")
    end
end

# axes with dimension argument: return range for specific dimension
function axes(arr, d::Int)
    s = size(arr)
    if d > length(s)
        return OneTo(1)
    end
    return OneTo(s[d])
end

# require_one_based_indexing: validate that arrays use ordinary 1-based axes.
# Based on Julia's base/abstractarray.jl helper used by packages before
# mutating or indexing user-provided arrays. sjulia currently supports
# one-based arrays/ranges, but keep the axes check so custom array-like values
# fail with the upstream-shaped ArgumentError instead of silently proceeding.
function require_one_based_indexing(A...)
    for a in A
        ax = axes(a)
        for r in ax
            if first(r) != 1
                throw(ArgumentError("offset arrays are not supported but got an array with index other than 1"))
            end
        end
    end
    return nothing
end

# Note: enumerate and zip are not yet fully implemented
# The Pure Julia iterator protocol (iterate function) is not supported
# These functions are exported but will cause compilation errors if used
# TODO: Implement when ArrayData::Tuple or iterator protocol is supported

# Placeholder functions - will error if called with unsupported patterns
# enumerate(arr) - works with eachindex pattern: for i in eachindex(arr)
# zip(a, b) - use manual indexing: for i in 1:min(length(a), length(b))

# =============================================================================
# Dimension permutation functions
# =============================================================================

# permutedims for 1D vector: converts to 1×N row vector
# permutedims for 2D matrix: transpose (swap rows and columns)
#
# Result allocation is routed through `similar(arr, ...)` so the public
# operation preserves the input element type rather than falling back to
# an untyped flat buffer (Issues #4018, #4656).
function permutedims(arr)
    s = size(arr)
    nd = length(s)
    if nd == 1
        n = s[1]
        result = similar(arr, 1, n)
        for i in 1:n
            result[1, i] = arr[i]
        end
        return result
    elseif nd == 2
        m = s[1]
        n = s[2]
        result = similar(arr, n, m)
        for i in 1:m
            for j in 1:n
                result[j, i] = arr[i, j]
            end
        end
        return result
    else
        error("permutedims without perm argument only supports 1D and 2D arrays")
    end
end

# permutedims with explicit permutation tuple
# Supports up to 4-dimensional arrays
function permutedims(arr, perm)
    s = size(arr)
    nd = length(s)

    # Validate permutation length matches array dimensions
    perm_len = length(perm)
    if perm_len != nd
        error("permutedims: permutation length must match array dimensions")
    end

    if nd == 2
        p1 = perm[1]
        p2 = perm[2]
        if p1 == 1 && p2 == 2
            # Identity permutation - copy
            m = s[1]
            n = s[2]
            if eltype(arr) === String
                return _array_string_copy_same_shape(arr)
            end
            result = similar(arr, m, n)
            for i in 1:m
                for j in 1:n
                    result[i, j] = arr[i, j]
                end
            end
            return result
        elseif p1 == 2 && p2 == 1
            # Transpose: delegate to the type-preserving `permutedims(arr)`
            # (Issue #3590).
            return permutedims(arr)
        else
            error("permutedims: invalid permutation indices")
        end
    elseif nd == 3
        # 3D array permutation
        p1 = Int64(perm[1])
        p2 = Int64(perm[2])
        p3 = Int64(perm[3])
        # New shape: (s[p1], s[p2], s[p3])
        ns1 = s[p1]
        ns2 = s[p2]
        ns3 = s[p3]
        result = similar(arr, ns1, ns2, ns3)
        # For each output index (i1, i2, i3), compute input index
        for i1 in 1:ns1
            for i2 in 1:ns2
                for i3 in 1:ns3
                    # Compute input indices using inverse permutation
                    # Output index (i1, i2, i3) maps to input at position where
                    # inp[pk] = ik for each dimension k
                    in1 = (p1 == 1 ? i1 : (p2 == 1 ? i2 : i3))
                    in2 = (p1 == 2 ? i1 : (p2 == 2 ? i2 : i3))
                    in3 = (p1 == 3 ? i1 : (p2 == 3 ? i2 : i3))
                    result[i1, i2, i3] = arr[in1, in2, in3]
                end
            end
        end
        return result
    elseif nd == 4
        # 4D array permutation
        p1 = Int64(perm[1])
        p2 = Int64(perm[2])
        p3 = Int64(perm[3])
        p4 = Int64(perm[4])
        # New shape
        ns1 = s[p1]
        ns2 = s[p2]
        ns3 = s[p3]
        ns4 = s[p4]
        result = similar(arr, ns1, ns2, ns3, ns4)
        for i1 in 1:ns1
            for i2 in 1:ns2
                for i3 in 1:ns3
                    for i4 in 1:ns4
                        # Compute input indices
                        in1 = (p1 == 1 ? i1 : (p2 == 1 ? i2 : (p3 == 1 ? i3 : i4)))
                        in2 = (p1 == 2 ? i1 : (p2 == 2 ? i2 : (p3 == 2 ? i3 : i4)))
                        in3 = (p1 == 3 ? i1 : (p2 == 3 ? i2 : (p3 == 3 ? i3 : i4)))
                        in4 = (p1 == 4 ? i1 : (p2 == 4 ? i2 : (p3 == 4 ? i3 : i4)))
                        result[i1, i2, i3, i4] = arr[in1, in2, in3, in4]
                    end
                end
            end
        end
        return result
    else
        error("permutedims: only supports arrays up to 4 dimensions")
    end
end

# permutedims!: permute dimensions of src and store result in dest
# Based on Julia's base/permuteddimsarray.jl
function permutedims!(dest, src, perm)
    s = size(src)
    nd = length(s)
    perm_len = length(perm)
    if perm_len != nd
        error("permutedims!: permutation length must match array dimensions")
    end
    if nd == 2
        p1 = perm[1]
        p2 = perm[2]
        m = s[1]
        n = s[2]
        if p1 == 1 && p2 == 2
            # Identity permutation - copy
            for i in 1:m
                for j in 1:n
                    dest[i, j] = src[i, j]
                end
            end
        elseif p1 == 2 && p2 == 1
            # Transpose
            for i in 1:m
                for j in 1:n
                    dest[j, i] = src[i, j]
                end
            end
        else
            error("permutedims!: invalid permutation indices")
        end
    elseif nd == 3
        p1 = Int64(perm[1])
        p2 = Int64(perm[2])
        p3 = Int64(perm[3])
        ns1 = s[p1]
        ns2 = s[p2]
        ns3 = s[p3]
        for i1 in 1:ns1
            for i2 in 1:ns2
                for i3 in 1:ns3
                    in1 = (p1 == 1 ? i1 : (p2 == 1 ? i2 : i3))
                    in2 = (p1 == 2 ? i1 : (p2 == 2 ? i2 : i3))
                    in3 = (p1 == 3 ? i1 : (p2 == 3 ? i2 : i3))
                    dest[i1, i2, i3] = src[in1, in2, in3]
                end
            end
        end
    else
        error("permutedims!: only supports 2D and 3D arrays")
    end
    return dest
end

# =============================================================================
# transpose and adjoint for arrays
# =============================================================================
# Based on Julia's LinearAlgebra module
# transpose(A) = permutedims(A) for 1D and 2D arrays
# adjoint(A) = conjugate transpose (conj applied element-wise, then transpose)

# transpose for arrays - pure permutation without conjugation
function transpose(arr::Array)
    return permutedims(arr)
end

# adjoint for arrays - conjugate transpose
# For real arrays, this is identical to transpose
# For complex arrays, each element is conjugated
# Type-preserving: Int64 input -> Int64 output
function adjoint(arr::Array)
    s = size(arr)
    nd = length(s)
    typ = eltype(arr)
    if nd == 1
        # 1D vector -> 1×N row vector (conjugated)
        n = s[1]
        if typ == Int64
            result = zeros(Int64, 1, n)
        elseif typ == Int8
            result = _array_undef_from_dims(Int8, (1, n))
        elseif typ == Int16
            result = _array_undef_from_dims(Int16, (1, n))
        elseif typ == Int32
            result = _array_undef_from_dims(Int32, (1, n))
        elseif typ == UInt8
            result = _array_undef_from_dims(UInt8, (1, n))
        elseif typ == UInt16
            result = _array_undef_from_dims(UInt16, (1, n))
        elseif typ == UInt32
            result = _array_undef_from_dims(UInt32, (1, n))
        elseif typ == UInt64
            result = _array_undef_from_dims(UInt64, (1, n))
        elseif typ == Float32
            result = _array_undef_from_dims(Float32, (1, n))
        elseif typ == Bool
            result = _array_undef_from_dims(Bool, (1, n))
        elseif typ == Complex{Float64}
            result = _array_undef_from_dims(Complex{Float64}, (1, n))
        else
            result = zeros(1, n)
        end
        for i in 1:n
            result[1, i] = conj(arr[i])
        end
        return result
    elseif nd == 2
        # 2D matrix -> conjugate transpose (n×m)
        m = s[1]
        n = s[2]
        if typ == Int64
            result = zeros(Int64, n, m)
        elseif typ == Int8
            result = _array_undef_from_dims(Int8, (n, m))
        elseif typ == Int16
            result = _array_undef_from_dims(Int16, (n, m))
        elseif typ == Int32
            result = _array_undef_from_dims(Int32, (n, m))
        elseif typ == UInt8
            result = _array_undef_from_dims(UInt8, (n, m))
        elseif typ == UInt16
            result = _array_undef_from_dims(UInt16, (n, m))
        elseif typ == UInt32
            result = _array_undef_from_dims(UInt32, (n, m))
        elseif typ == UInt64
            result = _array_undef_from_dims(UInt64, (n, m))
        elseif typ == Float32
            result = _array_undef_from_dims(Float32, (n, m))
        elseif typ == Bool
            result = _array_undef_from_dims(Bool, (n, m))
        elseif typ == Complex{Float64}
            result = _array_undef_from_dims(Complex{Float64}, (n, m))
        else
            result = zeros(n, m)
        end
        for i in 1:m
            for j in 1:n
                result[j, i] = conj(arr[i, j])
            end
        end
        return result
    else
        error("adjoint only supports 1D and 2D arrays")
    end
end

# =============================================================================
# adjoint for range types - convert to row vector
# =============================================================================
# Based on Julia's LinearAlgebra/src/adjtrans.jl
# Ranges are collected to arrays before applying adjoint

# adjoint for UnitRange - converts to 1×N row vector
function adjoint(r::UnitRange)
    return adjoint(collect(r))
end

# adjoint for LinRange - converts to 1×N row vector
function adjoint(r::LinRange{T}) where T
    n = r.len
    result = _array_undef_from_dims(T, (1, n))
    for i in 1:n
        value = r[i]
        result[1, i] = value
    end
    return result
end

# adjoint for StepRangeLen - converts to 1×N row vector
function adjoint(r::StepRangeLen{T}) where T
    n = length(r)
    result = _array_undef_from_dims(T, (1, n))
    for i in 1:n
        value = r[i]
        result[1, i] = value
    end
    return result
end

# adjoint for OneTo - converts to 1×N row vector
function adjoint(r::OneTo)
    return adjoint(collect(r))
end

# adjoint for the remaining range kinds — VM-native StepRange and the
# TwicePrecision-backed float StepRangeLen ranges that `a:s:b` and
# `range(start, stop; length)` produce (Issue #9419). Real elements, so
# adjoint == transpose == 1×N row vector, like the methods above.
function adjoint(r::AbstractRange)
    return adjoint(collect(r))
end

# =============================================================================
# transpose for range types - convert to row vector (no conjugation)
# =============================================================================

# transpose for UnitRange - converts to 1×N row vector
function transpose(r::UnitRange)
    return transpose(collect(r))
end

# transpose for LinRange - converts to 1×N row vector
function transpose(r::LinRange)
    return transpose(collect(r))
end

# transpose for StepRangeLen - converts to 1×N row vector
function transpose(r::StepRangeLen)
    return transpose(collect(r))
end

# transpose for OneTo - converts to 1×N row vector
function transpose(r::OneTo)
    return transpose(collect(r))
end

# transpose for the remaining range kinds (VM-native StepRange / float
# StepRangeLen ranges; Issue #9419) - converts to 1×N row vector
function transpose(r::AbstractRange)
    return transpose(collect(r))
end

# =============================================================================
# Array repetition functions
# =============================================================================

# Note: repeat(arr::Array, n::Int) is already defined above.
# String repeat is handled by Pure Julia in base/strings/basic.jl

# repeat(v, m, n) for 1D vector - create m×n matrix by tiling
# Example: repeat([1, 2], 3, 2) => 6×2 matrix
#   1  1
#   2  2
#   1  1
#   2  2
#   1  1
#   2  2
function repeat(arr::Array, m::Int, n::Int)
    # Check dimensionality using length(size(arr))
    # 1D: size(arr) = (n,), length = 1
    # 2D: size(arr) = (m, n), length = 2
    dims = length(size(arr))
    if dims == 1
        # 1D vector: repeat vertically m times, horizontally n times
        len = length(arr)
        result = similar(arr, len * m, n)
        for j in 1:n
            row = 1
            for _ in 1:m
                for i in 1:len
                    result[row, j] = arr[i]
                    row = row + 1
                end
            end
        end
        return result
    else
        # 2D matrix: repeat m times vertically, n times horizontally
        rows = size(arr, 1)
        cols = size(arr, 2)
        result = similar(arr, rows * m, cols * n)
        for block_j in 0:(n-1)
            for block_i in 0:(m-1)
                for j in 1:cols
                    for i in 1:rows
                        result[block_i * rows + i, block_j * cols + j] = arr[i, j]
                    end
                end
            end
        end
        return result
    end
end

# =============================================================================
# empty - create empty collection of same type
# =============================================================================
# Based on Julia's base/abstractarray.jl and base/abstractdict.jl
#
# empty(a) creates an empty collection of the same type as a

# empty for arrays - create empty array of same element type
function empty(arr::Array{T}) where T
    return _array_undef_from_dims(T, (0,))
end

# empty(a, T) creates empty array with element type T
function empty(arr::Array, ::Type{S}) where S
    return _array_undef_from_dims(S, (0,))
end

# empty for Dict - create empty Dict with same key/value types
# Note: In SubsetJuliaVM, Dict type information is limited
# This is a simplified implementation
function empty(dict::Dict)
    return Dict()
end

# empty for Dict with value type specified
function empty(dict::Dict, V)
    return Dict()
end

# empty for Tuple - return empty tuple
function empty(tup::Tuple)
    return ()
end

# =============================================================================
# Bounds checking
# =============================================================================
# Based on Julia's base/abstractarray.jl

# checkbounds(Bool, A, i) - return true if index i is valid for array A
function checkbounds(::Type{Bool}, A, i::Int)
    return 1 <= i && i <= length(A)
end

# checkbounds(Bool, A, i) - fallback for non-Int64 indices
function checkbounds(::Type{Bool}, A, i)
    return checkbounds(Bool, A, Int64(i))
end

# checkbounds(A, i) - throw BoundsError if index i is not valid for array A
function checkbounds(A, i::Int)
    if !(1 <= i <= length(A))
        throw(BoundsError(A, i))
    end
    return nothing
end

# checkbounds(A, i) - fallback for non-Int64 indices
function checkbounds(A, i)
    return checkbounds(A, Int64(i))
end

# checkindex(Bool, inds, i) - check if index i is within range inds
function checkindex(::Type{Bool}, inds, i::Int)
    # For UnitRange (1:n style), use firstindex/lastindex
    first_idx = inds[1]
    last_idx = inds[length(inds)]
    return first_idx <= i && i <= last_idx
end

# checkindex(Bool, inds, i) - fallback for non-Int64 indices
function checkindex(::Type{Bool}, inds, i)
    return checkindex(Bool, inds, Int64(i))
end

# =============================================================================
# isassigned - check if array index has an assigned value (Issue #1836)
# =============================================================================
# Based on Julia's base/essentials.jl:1007-1038 and base/array.jl:229-242
#
# In SubsetJuliaVM, all array elements for isbits types (Int64, Float64, Bool,
# etc.) are always assigned, so isassigned simplifies to a bounds check.

function isassigned(a, i::Int)
    return 1 <= i && i <= length(a)
end

function isassigned(a, i::Integer)
    return isassigned(a, Int64(i))
end

# =============================================================================
# popat! - remove and return element at index
# =============================================================================
# Based on Julia's base/array.jl:1710-1725
#
# popat!(a, i) removes and returns the element at index i
# popat!(a, i, default) returns default if index is out of bounds

function popat!(a, i::Int)
    x = a[i]
    deleteat!(a, i)
    return x
end

function popat!(a, i::Int, default)
    n = length(a)
    if 1 <= i && i <= n
        x = a[i]
        deleteat!(a, i)
        return x
    else
        return default
    end
end

# =============================================================================
# Boolean array construction functions
# =============================================================================
# Based on Julia's base/bitarray.jl:393-416
#
# In Julia, trues/falses return BitArray (compact boolean storage). SubsetJuliaVM
# allocates Bool values through the ordinary Array helper, then `_mark_bitarray`
# converts the VM backing storage to the bit-packed BitArray-family representation.

function trues(dims...)
    result = _array_undef_from_dims(Bool, dims)
    fill!(result, true)
    return _mark_bitarray(result)
end

function falses(dims...)
    result = _array_undef_from_dims(Bool, dims)
    fill!(result, false)
    return _mark_bitarray(result)
end

# =============================================================================
# fill - create array filled with a value
# =============================================================================
# Based on Julia's base/array.jl
#
# fill(value, dims...) creates an array filled with the given value.
# The element type is determined by the value's type.

function fill(value, dims::Tuple)
    T = typeof(value)
    result = _array_undef_from_dims(T, dims)
    fill!(result, value)
    return result
end

function fill(value, len::Int)
    T = typeof(value)
    result = _array_undef_from_dims(T, (len,))
    fill!(result, value)
    return result
end

function fill(value, rows::Int, cols::Int)
    T = typeof(value)
    result = _array_undef_from_dims(T, (rows, cols))
    fill!(result, value)
    return result
end

function fill(value, d1::Int, d2::Int, d3::Int)
    T = typeof(value)
    result = _array_undef_from_dims(T, (d1, d2, d3))
    fill!(result, value)
    return result
end

function _array_undef_from_dims(typ::Type{Float64}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Float64}, dims)
end

function _array_undef_from_dims(typ::Type{Float32}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Float32}, dims)
end

function _array_undef_from_dims(typ::Type{Int64}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Int}, dims)
end

function _array_undef_from_dims(typ::Type{Int8}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Int8}, dims)
end

function _array_undef_from_dims(typ::Type{Int16}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Int16}, dims)
end

function _array_undef_from_dims(typ::Type{Int32}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Int32}, dims)
end

function _array_undef_from_dims(typ::Type{Int128}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Int128}, dims)
end

function _array_undef_from_dims(typ::Type{UInt8}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{UInt8}, dims)
end

function _array_undef_from_dims(typ::Type{UInt16}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{UInt16}, dims)
end

function _array_undef_from_dims(typ::Type{UInt32}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{UInt32}, dims)
end

function _array_undef_from_dims(typ::Type{UInt64}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{UInt64}, dims)
end

function _array_undef_from_dims(typ::Type{UInt128}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{UInt128}, dims)
end

function _array_undef_from_dims(typ::Type{Bool}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Bool}, dims)
end

function _array_undef_from_dims(typ::Type{String}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{String}, dims)
end

function _array_undef_from_dims(typ::Type{Char}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Char}, dims)
end

function _array_undef_from_dims(typ::Type{Complex{Float64}}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Complex{Float64}}, dims)
end

function _array_undef_from_dims(typ::Type{Pair}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Pair}, dims)
end

function _array_undef_from_dims(typ::Type{Pair{K,V}}, dims::Tuple) where {K,V}
    _array_check_undef_dims(dims)
    return similar(Array{Pair{K,V}}, dims)
end

function _array_undef_from_dims(typ::Type{Number}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Number}, dims)
end

function _array_undef_from_dims(typ::Type{Real}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Real}, dims)
end

function _array_undef_from_dims(typ::Type{Integer}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Integer}, dims)
end

function _array_undef_from_dims(typ::Type{Signed}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Signed}, dims)
end

function _array_undef_from_dims(typ::Type{Unsigned}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Unsigned}, dims)
end

function _array_undef_from_dims(typ::Type{AbstractFloat}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{AbstractFloat}, dims)
end

function _array_undef_from_dims(typ::Type{Any}, dims::Tuple)
    _array_check_undef_dims(dims)
    return similar(Array{Any}, dims)
end

function _array_undef_from_dims(typ::Type{T}, dims::Tuple) where T
    _array_check_undef_dims(dims)
    return similar(Array{T}, dims)
end

function _array_check_undef_dims(dims::Tuple)
    if length(dims) == 1
        d1 = Int64(dims[1])
        if d1 < 0
            throw(ArgumentError("invalid GenericMemory size: the number of elements is either negative or too large for system address width"))
        end
    else
        for d in dims
            di = Int64(d)
            if di < 0
                throw(ArgumentError("invalid Array dimensions"))
            end
        end
    end
    return dims
end

function zeros(dims::Tuple)
    return zeros(Float64, dims)
end

function zeros(dims::Int...)
    return zeros(Float64, dims)
end

function zeros(typ::Type{T}, dims::Tuple) where T
    result = _array_undef_from_dims(T, dims)
    fill!(result, zero(T))
    return result
end

function zeros(typ::Type{T}, dims...) where T
    result = _array_undef_from_dims(T, dims)
    fill!(result, zero(T))
    return result
end

function ones(dims::Tuple)
    return ones(Float64, dims)
end

function ones(dims::Int...)
    return ones(Float64, dims)
end

function ones(typ::Type{T}, dims::Tuple) where T
    result = _array_undef_from_dims(T, dims)
    fill!(result, one(T))
    return result
end

function ones(typ::Type{T}, dims...) where T
    result = _array_undef_from_dims(T, dims)
    fill!(result, one(T))
    return result
end

# =============================================================================
# resize! - Resize vector to new length
# =============================================================================
# Based on Julia's base/array.jl:1533
#
# resize!(a, n) resizes collection a to contain n elements.
# If n is smaller than the current size, the first n elements are retained.
# If n is larger, the collection is extended with uninitialized values
# (zeros for numeric types, false for Bool).

function resize!(a::Array, n::Int)
    current = length(a)
    if n < 0
        error("resize!: new length must be ≥ 0")
    end
    if n > current
        # Grow: push default values (zeros)
        # Get element type and push appropriate default.
        # Avoid storing the default in a branch-assigned local: sjulia's current
        # join can widen that binding and push 0.0 into Bool arrays (Issue #4311).
        et = eltype(a)
        i = current
        if et == Bool
            while i < n
                push!(a, false)
                i = i + 1
            end
        elseif et == Int64
            while i < n
                push!(a, Int64(0))
                i = i + 1
            end
        else
            while i < n
                push!(a, 0.0)
                i = i + 1
            end
        end
    elseif n < current
        # Shrink: pop elements from end. ArrayPop handles both the legacy
        # native-array carrier and Pure Julia Array wrappers (Issue #4570).
        i = current
        while i > n
            pop!(a)
            i = i - 1
        end
    end
    return a
end

# =============================================================================
# keepat! - Keep only elements at specified indices
# =============================================================================
# Based on Julia's base/array.jl:3078
#
# keepat!(a, inds) removes items at all indices NOT in inds, and returns
# the modified array. Items are shifted to fill gaps.
# inds must be sorted and unique integer indices.
# keepat!(a, m::Vector{Bool}) keeps elements where m[i] is true.

function keepat!(a::Array, inds)
    # Check if inds is a boolean mask
    if length(inds) > 0 && eltype(inds) == Bool
        # Boolean mask version
        if length(inds) != length(a)
            error("keepat!: mask length must match array length")
        end

        j = 1
        for i in 1:length(a)
            # Direct truthiness check works for Bool values
            if inds[i]
                if j != i
                    a[j] = a[i]
                end
                j = j + 1
            end
        end

        # Remove remaining elements
        resize!(a, j - 1)
        return a
    else
        # Integer indices version
        # Validate that indices are sorted and unique
        n_keep = length(inds)
        if n_keep > 0
            prev = inds[1]
            for i in 2:n_keep
                curr = inds[i]
                if curr <= prev
                    error("keepat!: indices must be unique and sorted")
                end
                prev = curr
            end
        end

        # Move elements to keep to the front
        j = 1
        for k in inds
            if k < 1 || k > length(a)
                error("keepat!: index out of bounds")
            end
            if j != k
                a[j] = a[k]
            end
            j = j + 1
        end

        # Remove remaining elements from the end
        resize!(a, n_keep)
        return a
    end
end

# =============================================================================
# pushfirst! - Insert element at beginning of array
# =============================================================================
# Based on Julia's base/array.jl:1746
#
# pushfirst!(a, item) inserts item at the beginning of a and returns a.
# All existing elements are shifted to the right by one position.

function pushfirst!(a::Array, item)
    n = length(a)
    # Extend array by one element
    resize!(a, n + 1)
    # Shift all elements to the right
    i = n + 1
    while i > 1
        a[i] = a[i - 1]
        i = i - 1
    end
    # Insert item at position 1
    a[1] = item
    return a
end

# =============================================================================
# popfirst! - Remove and return first element
# =============================================================================
# Based on Julia's base/array.jl:1805
#
# popfirst!(a) removes the first element from a and returns it.
# Throws an error if array is empty.

function popfirst!(a::Array)
    n = length(a)
    if n == 0
        error("popfirst!: array must be non-empty")
    end
    # Get first element
    item = a[1]
    # Shift all elements to the left
    i = 1
    while i < n
        a[i] = a[i + 1]
        i = i + 1
    end
    # Shrink array
    resize!(a, n - 1)
    return item
end

# =============================================================================
# insert! - Insert element at specific index
# =============================================================================
# Based on Julia's base/array.jl:1830
#
# insert!(a, index, item) inserts item into a at the given index.
# index is the index of item in the resulting array.

function insert!(a::Array, index::Int, item)
    n = length(a)
    if index < 1 || index > n + 1
        error("insert!: index out of bounds")
    end
    # Extend array by one element
    resize!(a, n + 1)
    # Shift elements from index to end to the right
    i = n + 1
    while i > index
        a[i] = a[i - 1]
        i = i - 1
    end
    # Insert item at index
    a[index] = item
    return a
end

# =============================================================================
# append! - Append elements from collection to array
# =============================================================================
# Based on Julia's base/array.jl:1408
#
# append!(a, items) adds all items from collection to the end of a.
# Returns the modified array.

function append!(a::Array, items)
    for item in items
        push!(a, item)
    end
    return a
end

# =============================================================================
# prepend! - Prepend elements from collection to array
# =============================================================================
# Based on Julia's base/array.jl:1428
#
# prepend!(a, items) adds all items from collection to the beginning of a.
# Returns the modified array.

function prepend!(a::Array, items)
    # Collect items first to get length (since items may be an iterator)
    items_arr = collect(items)
    m = length(items_arr)
    if m == 0
        return a
    end
    n = length(a)
    # Extend array
    resize!(a, n + m)
    # Shift existing elements to the right by m positions
    i = n + m
    while i > m
        a[i] = a[i - m]
        i = i - 1
    end
    # Copy items to the beginning
    for i in 1:m
        a[i] = items_arr[i]
    end
    return a
end

# =============================================================================
# deleteat! - Delete element at index
# =============================================================================
# Based on Julia's base/array.jl:1880
#
# deleteat!(a, i) removes the element at index i from a.
# Returns the modified array.

function deleteat!(a::Array, i::Int)
    n = length(a)
    if i < 1 || i > n
        error("deleteat!: index out of bounds")
    end
    # Shift elements to the left
    j = i
    while j < n
        a[j] = a[j + 1]
        j = j + 1
    end
    # Shrink array
    resize!(a, n - 1)
    return a
end

# deleteat!(a, inds) removes the elements at the (1-based) indices in `inds`
# (a vector or range). Used when the compiled `ArrayDeleteAtIndices` fast path
# falls back to method dispatch for a `copy`/`collect` result (Issue #5744).
# `inds` is intentionally untyped: the runtime builtin-fallback matcher refuses
# to match a native-array argument against `::AbstractVector` (the #4189 guard),
# so a typed parameter would never be selected — `::Any` is, while the scalar
# `deleteat!(a::Array, i::Int)` stays more specific for a single index.
# Delete in descending order so earlier removals don't shift later indices.
function deleteat!(a::Array, inds)
    idxs = Int64[]
    for j in inds
        push!(idxs, Int64(j))
    end
    sort!(idxs)
    # Delete in descending order so earlier removals don't shift later indices,
    # skipping duplicate indices.
    k = length(idxs)
    prev = 0
    while k >= 1
        i = idxs[k]
        if i != prev
            deleteat!(a, i)
            prev = i
        end
        k -= 1
    end
    return a
end

# =============================================================================
# indexin - Find indices of first collection in second
# =============================================================================
# Based on Julia's base/array.jl:2861
#
# indexin(a, b) returns an array containing the first index in b for each
# value in a that is a member of b. Returns nothing for elements not found.

function indexin(a, b)
    # Build a dictionary for O(1) lookup
    bdict = Dict{Any, Int64}()
    for i in 1:length(b)
        val = b[i]
        if !haskey(bdict, val)
            bdict[val] = i
        end
    end

    # Look up each element of a. Upstream Julia returns
    # Vector{Union{Nothing, Int64}} for one-based array keys.
    result = Vector{Union{Nothing, Int64}}(undef, length(a))
    idx = 1
    for val in a
        if haskey(bdict, val)
            result[idx] = bdict[val]
        else
            result[idx] = nothing
        end
        idx += 1
    end
    return result
end

# =============================================================================
# Permutation functions
# =============================================================================
# Based on Julia's base/combinatorics.jl

# isperm(p) - Check if p is a valid permutation of 1:n
# A valid permutation contains each integer from 1 to n exactly once
function isperm(p)
    n = length(p)
    if n == 0
        return true
    end

    # Use a boolean array to track which values we've seen
    seen = falses(n)
    for val in p
        # Check if val is a valid integer in range 1:n
        if !isa(val, Int64) && !isa(val, Int)
            return false
        end
        idx = Int64(val)
        if idx < 1 || idx > n
            return false
        end
        # Check if we've already seen this value
        if seen[idx]  # Direct truthiness check
            return false
        end
        seen[idx] = true
    end
    return true
end

# invperm(p) - Compute the inverse permutation
# If p is a permutation, then invperm(p)[p[i]] == i for all i
function invperm(p)
    n = length(p)
    if !isperm(p)
        error("invperm: argument is not a permutation")
    end

    # Create result array
    result = zeros(Int64, n)
    for i in 1:n
        result[p[i]] = i
    end
    return result
end

# =============================================================================
# empty! - Remove all elements from a collection
# =============================================================================
# Based on Julia's base/array.jl:2124
#
# empty!(a) removes all elements from a and returns the modified collection.

function empty!(a::Array)
    resize!(a, 0)
    return a
end

# =============================================================================
# filter! - Filter array elements in place
# =============================================================================
# Based on Julia's base/array.jl:3035
#
# filter!(f, a) removes elements from a for which f returns false.
# Returns the modified array.

function filter!(f, a::Array)
    j = 1
    for i in 1:length(a)
        ai = a[i]
        if f(ai)
            if j != i
                a[j] = ai
            end
            j = j + 1
        end
    end
    # Resize to keep only the elements that passed the filter
    resize!(a, j - 1)
    return a
end

# =============================================================================
# splice! - Remove and optionally replace elements
# =============================================================================
# Based on Julia's base/array.jl:2039
#
# splice!(a, i) removes element at index i and returns it.
# splice!(a, i, v) replaces element at index i with v and returns old element.
# splice!(a, r) removes elements in range r and returns them.
# splice!(a, r, ins) removes elements in range r, inserts ins, returns old elements.

# splice!(a, i) - Remove and return element at index i
function splice!(a::Array, i::Int)
    n = length(a)
    if i < 1 || i > n
        error("splice!: index out of bounds")
    end
    # Save the element to return
    v = a[i]
    # Shift elements left
    j = i
    while j < n
        a[j] = a[j + 1]
        j = j + 1
    end
    # Shrink array
    resize!(a, n - 1)
    return v
end

# splice!(a, i, replacement) - Replace element at index i with replacement
# Note: replacement can be a single value or an array of values
function splice!(a::Array, i::Int, replacement)
    n = length(a)
    if i < 1 || i > n
        error("splice!: index out of bounds")
    end

    # Save the old element
    v = a[i]

    # Check if replacement is an array
    if isa(replacement, Array)
        items_arr = replacement
        m = length(items_arr)
        if m == 0
            # Remove element without replacement
            j = i
            while j < n
                a[j] = a[j + 1]
                j = j + 1
            end
            resize!(a, n - 1)
        elseif m == 1
            # Single element replacement
            a[i] = items_arr[1]
        else
            # Multiple element replacement: need to grow array
            resize!(a, n + m - 1)
            # Shift elements right
            j = n + m - 1
            while j > i + m - 1
                a[j] = a[j - m + 1]
                j = j - 1
            end
            # Insert new elements
            for k in 1:m
                a[i + k - 1] = items_arr[k]
            end
        end
    else
        # Single value replacement
        a[i] = replacement
    end

    return v
end

# splice!(a, r::UnitRange) - Remove and return elements in range r (Issue #3481)
function splice!(a::Array, r::UnitRange)
    n = length(a)
    i_start = first(r)
    i_end = last(r)
    if i_start < 1 || i_end > n
        error("splice!: range out of bounds")
    end
    # Collect removed elements
    removed = a[r]
    len_removed = i_end - i_start + 1
    # Shift elements left
    j = i_start
    while j <= n - len_removed
        a[j] = a[j + len_removed]
        j = j + 1
    end
    resize!(a, n - len_removed)
    return removed
end

# splice!(a, r::UnitRange, ins) - Remove elements in range r, insert ins, return old elements (Issue #3481)
function splice!(a::Array, r::UnitRange, ins)
    n = length(a)
    i_start = first(r)
    i_end = last(r)
    if i_start < 1 || i_end > n
        error("splice!: range out of bounds")
    end
    removed = a[r]
    len_removed = i_end - i_start + 1
    # Build new array: a[1:i_start-1] + ins + a[i_end+1:n]
    if isa(ins, Array)
        ins_arr = ins
    else
        ins_arr = [ins]
    end
    m = length(ins_arr)
    new_len = n - len_removed + m
    # Shift tail
    if m >= len_removed
        resize!(a, n + m - len_removed)
        j = n + m - len_removed
        while j > i_start + m - 1
            a[j] = a[j - m + len_removed]
            j = j - 1
        end
    else
        j = i_start + m
        while j <= n - len_removed + m
            a[j] = a[j + len_removed - m]
            j = j + 1
        end
        resize!(a, new_len)
    end
    for k in 1:m
        a[i_start + k - 1] = ins_arr[k]
    end
    return removed
end

# =============================================================================
# map! - Apply function to array elements in-place
# =============================================================================
# Based on Julia's base/abstractarray.jl:3381 and 3426
#
# map!(f, dest, A) applies f to each element of A and stores result in dest.
# map!(f, A) applies f to each element of A in-place (modifies A).

# map!(f, A) - Apply f to A in-place (simple 2-arg version)
function map!(f, a::Array)
    for i in 1:length(a)
        ai = a[i]
        a[i] = f(ai)
    end
    return a
end

# map!(f, dest, A) - Apply f to A and store in dest (3-arg version)
# Note: In Julia, map! processes min(length(dest), length(src)) elements
# and doesn't resize the destination array
function map!(f, dest::Array, src::Array)
    n = min(length(dest), length(src))
    for i in 1:n
        ai = src[i]
        dest[i] = f(ai)
    end
    return dest
end

function map!(f, dest::Array, A::Array, B::Array)
    for (i, j, k) in zip(eachindex(dest), eachindex(A), eachindex(B))
        a = A[j]
        b = B[k]
        dest[i] = f(a, b)
    end
    return dest
end

function map!(f, dest::Array, A::Array, B::Array, C::Array)
    for (i, j, k, l) in zip(eachindex(dest), eachindex(A), eachindex(B), eachindex(C))
        a = A[j]
        b = B[k]
        c = C[l]
        dest[i] = f(a, b, c)
    end
    return dest
end

function map!(f, dest::Array, A::Array, B::Array, C::Array, D::Array)
    n = min(length(dest), length(A))
    n = min(n, length(B))
    n = min(n, length(C))
    n = min(n, length(D))
    for i in 1:n
        a = A[i]
        b = B[i]
        c = C[i]
        d = D[i]
        dest[i] = f(a, b, c, d)
    end
    return dest
end

function map!(f, dest::Array, A::Array, B::Array, C::Array, D::Array, E::Array)
    n = min(length(dest), length(A))
    n = min(n, length(B))
    n = min(n, length(C))
    n = min(n, length(D))
    n = min(n, length(E))
    for i in 1:n
        a = A[i]
        b = B[i]
        c = C[i]
        d = D[i]
        e = E[i]
        dest[i] = f(a, b, c, d, e)
    end
    return dest
end

function map!(f, dest::Array, As::Array...)
    nsrc = length(As)
    if nsrc == 0
        return map!(f, dest)
    end

    n = length(dest)
    for j in 1:nsrc
        n = min(n, length(As[j]))
    end

    for i in 1:n
        if nsrc == 1
            dest[i] = f(As[1][i])
        elseif nsrc == 2
            dest[i] = f(As[1][i], As[2][i])
        elseif nsrc == 3
            dest[i] = f(As[1][i], As[2][i], As[3][i])
        elseif nsrc == 4
            dest[i] = f(As[1][i], As[2][i], As[3][i], As[4][i])
        elseif nsrc == 5
            dest[i] = f(As[1][i], As[2][i], As[3][i], As[4][i], As[5][i])
        elseif nsrc == 6
            dest[i] = f(As[1][i], As[2][i], As[3][i], As[4][i], As[5][i], As[6][i])
        elseif nsrc == 7
            dest[i] = f(As[1][i], As[2][i], As[3][i], As[4][i], As[5][i], As[6][i], As[7][i])
        elseif nsrc == 8
            dest[i] = f(As[1][i], As[2][i], As[3][i], As[4][i], As[5][i], As[6][i], As[7][i], As[8][i])
        else
            values = Any[]
            for j in 1:nsrc
                push!(values, As[j][i])
            end
            dest[i] = f(values...)
        end
    end
    return dest
end

# =============================================================================
# clamp! - Clamp array values in place
# =============================================================================
# Based on Julia's base/math.jl clamp! function
#
# clamp!(a, lo, hi) restricts each element of a to the interval [lo, hi].
# Values less than lo are set to lo, values greater than hi are set to hi.

"""
    clamp!(a, lo, hi)

Restrict values in array `a` to the interval [`lo`, `hi`], in-place.
For each element, values less than `lo` will become `lo` and values greater
than `hi` will become `hi`.

Returns the modified array `a`.

# Examples
```julia
julia> a = [1.0, 5.0, 10.0, 15.0];
julia> clamp!(a, 3, 12);
julia> a
4-element Vector{Float64}:
  3.0
  5.0
 10.0
 12.0
```
"""
function clamp!(a, lo, hi)
    n = length(a)
    for i in 1:n
        x = a[i]
        if x < lo
            a[i] = lo
        elseif x > hi
            a[i] = hi
        end
    end
    return a
end

# =============================================================================
# cat: general array concatenation along a specified dimension
# =============================================================================
# Based on Julia's Base.cat

# cat(A, B; dims): concatenate two arrays along dimension dims
# dims=1: vertical concatenation (like vcat for matrices)
# dims=2: horizontal concatenation (like hcat for matrices)
function _cat_result_like(A, B, dims::Tuple)
    if eltype(A) == eltype(B)
        return similar(A, dims)
    end
    return _array_undef_from_dims(promote_type(eltype(A), eltype(B)), dims)
end

function _cat_result_like(A, B, d1::Int)
    if eltype(A) == eltype(B)
        return similar(A, d1)
    end
    return _array_undef_from_dims(promote_type(eltype(A), eltype(B)), (d1,))
end

function _cat_result_like(A, B, d1::Int, d2::Int)
    if eltype(A) == eltype(B)
        return similar(A, d1, d2)
    end
    return _array_undef_from_dims(promote_type(eltype(A), eltype(B)), (d1, d2))
end

function cat(A, B; dims)
    sA = size(A)
    sB = size(B)
    ndA = length(sA)
    ndB = length(sB)
    if dims == 1
        if ndA == 1 && ndB == 1
            # Both 1D: concatenate elements
            na = length(A)
            nb = length(B)
            result = _cat_result_like(A, B, na + nb)
            for i in 1:na
                result[i] = A[i]
            end
            for i in 1:nb
                result[na + i] = B[i]
            end
            return result
        else
            # 2D: vertical concatenation
            mA = size(A, 1)
            nA = size(A, 2)
            mB = size(B, 1)
            nB = size(B, 2)
            if nA != nB
                error("cat: dimension mismatch along dim 2: $nA vs $nB")
            end
            result = _cat_result_like(A, B, mA + mB, nA)
            for i in 1:mA
                for j in 1:nA
                    result[i, j] = A[i, j]
                end
            end
            for i in 1:mB
                for j in 1:nB
                    result[mA + i, j] = B[i, j]
                end
            end
            return result
        end
    elseif dims == 2
        if ndA == 1 && ndB == 1
            # Both 1D: treat as column vectors, produce matrix
            na = length(A)
            nb = length(B)
            if na != nb
                error("cat: dimension mismatch along dim 1: $na vs $nb")
            end
            result = _cat_result_like(A, B, na, 2)
            for i in 1:na
                result[i, 1] = A[i]
                result[i, 2] = B[i]
            end
            return result
        else
            # 2D: horizontal concatenation
            mA = size(A, 1)
            nA = size(A, 2)
            mB = size(B, 1)
            nB = size(B, 2)
            if mA != mB
                error("cat: dimension mismatch along dim 1: $mA vs $mB")
            end
            result = _cat_result_like(A, B, mA, nA + nB)
            for i in 1:mA
                for j in 1:nA
                    result[i, j] = A[i, j]
                end
            end
            for i in 1:mB
                for j in 1:nB
                    result[i, nA + j] = B[i, j]
                end
            end
            return result
        end
    else
        error("cat: dims must be 1 or 2 for 2D arrays")
    end
end

# =============================================================================
# mapslices: apply a function to slices of an array along a dimension
# =============================================================================
# Based on Julia's Base.mapslices

# mapslices(f, A; dims): apply f to each slice of A along dimension dims
# For 2D matrices:
#   dims=1: apply f to each column (slices along rows)
#   dims=2: apply f to each row (slices along columns)
function _mapslices_column(A, m::Int, j::Int)
    col = similar(A, m)
    for i in 1:m
        col[i] = A[i, j]
    end
    return col
end

function _mapslices_row(A, n::Int, i::Int)
    row = similar(A, n)
    for j in 1:n
        row[j] = A[i, j]
    end
    return row
end

function mapslices(f, A; dims)
    m = size(A, 1)
    n = size(A, 2)
    if dims == 1
        # Apply f to each column
        if n == 0
            return _array_undef_from_dims(Float64, (1, 0))
        end
        first_value = f(_mapslices_column(A, m, 1))
        results = _array_undef_from_dims(typeof(first_value), (1, n))
        results[1, 1] = first_value
        for j in 2:n
            results[1, j] = f(_mapslices_column(A, m, j))
        end
        return results
    elseif dims == 2
        # Apply f to each row
        if m == 0
            return _array_undef_from_dims(Float64, (0, 1))
        end
        first_value = f(_mapslices_row(A, n, 1))
        results = _array_undef_from_dims(typeof(first_value), (m, 1))
        results[1, 1] = first_value
        for i in 2:m
            results[i, 1] = f(_mapslices_row(A, n, i))
        end
        return results
    else
        error("mapslices: dims must be 1 or 2 for 2D arrays")
    end
end

# sortslices: sort slices of an array along a dimension
# dims=1: sort rows by comparing row vectors lexicographically
# dims=2: sort columns by comparing column vectors lexicographically
function sortslices(A; dims)
    m = size(A, 1)
    n = size(A, 2)
    if dims == 1
        # Sort rows: compare row i and row j lexicographically
        # Create index array and sort it
        idx = collect(1:m)
        # Insertion sort on indices
        for i in 2:m
            key = idx[i]
            j = i - 1
            while j >= 1
                # Compare row idx[j] vs row key lexicographically
                should_swap = false
                for col in 1:n
                    if A[idx[j], col] > A[key, col]
                        should_swap = true
                        break
                    elseif A[idx[j], col] < A[key, col]
                        break
                    end
                end
                if !should_swap
                    break
                end
                idx[j + 1] = idx[j]
                j = j - 1
            end
            idx[j + 1] = key
        end
        # Build result matrix with sorted rows
        result = similar(A, m, n)
        for i in 1:m
            for j in 1:n
                result[i, j] = A[idx[i], j]
            end
        end
        return result
    elseif dims == 2
        # Sort columns: compare column i and column j lexicographically
        idx = collect(1:n)
        for i in 2:n
            key = idx[i]
            j = i - 1
            while j >= 1
                should_swap = false
                for row in 1:m
                    if A[row, idx[j]] > A[row, key]
                        should_swap = true
                        break
                    elseif A[row, idx[j]] < A[row, key]
                        break
                    end
                end
                if !should_swap
                    break
                end
                idx[j + 1] = idx[j]
                j = j - 1
            end
            idx[j + 1] = key
        end
        # Build result matrix with sorted columns
        result = similar(A, m, n)
        for i in 1:m
            for j in 1:n
                result[i, j] = A[i, idx[j]]
            end
        end
        return result
    else
        error("sortslices: dims must be 1 or 2 for 2D arrays")
    end
end

# =============================================================================
# findnext / findprev with predicate function (Issue #2109)
# =============================================================================
# Based on Julia's base/array.jl
# findnext(testf, A, start) - find next index >= start where testf(A[i]) is true
# findprev(testf, A, start) - find prev index <= start where testf(A[i]) is true

function findnext(testf::Function, A, start::Int)
    n = length(A)
    i = start
    while i <= n
        if testf(A[i])
            return i
        end
        i = i + 1
    end
    return nothing
end

function findprev(testf::Function, A, start::Int)
    i = start
    while i >= 1
        if testf(A[i])
            return i
        end
        i = i - 1
    end
    return nothing
end
