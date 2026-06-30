# =============================================================================
# genericmemory.jl - Memory{T} typed memory buffer
# =============================================================================
# Based on Julia's base/genericmemory.jl
#
# In Julia 1.11+, Memory{T} is an alias for GenericMemory{:not_atomic, T, Core.CPU}.
# It is a low-level fixed-size typed buffer used internally by Vector, Dict, etc.
#
# SubsetJuliaVM implementation: Memory{T} is a native Rust primitive type.
# Constructor: Memory{T}(n) creates a typed buffer of length n.
# Builtin support: length, getindex, setindex! are handled natively.
#
# This file provides Pure Julia functions that work on top of the native Memory type:
# - size(m::Memory), ndims(m::Memory): DenseVector shape protocol
# - similar(m::Memory{T}, ...): Memory/Array allocation following GenericMemory
# - copy(m::Memory{T}): shallow type-preserving copy
# - parent(ref), memoryindex(ref): MemoryRef accessors matching Julia Base

function parent(ref::MemoryRef)
    return memoryrefparent(ref)
end

function memoryindex(ref::MemoryRef)
    return memoryrefoffset(ref)
end

# =============================================================================
# Shape protocol
# =============================================================================

function size(m::Memory)
    return (length(m),)
end

function size(m::Memory, d::Int64)
    if d < 1
        error("dimension out of range")
    elseif d == 1
        return length(m)
    end
    return 1
end

function ndims(m::Memory)
    return 1
end

function eltype(m::Memory)
    return typeof(m).parameters[1]
end

# =============================================================================
# show - compact display
# =============================================================================
# `show(io, m::Memory)` writes the compact `[a, b, c]` / `T[]` form, matching
# upstream `show(io, ::GenericMemory)` (Issue #6697). Without this method,
# `repr(m)` (= `sprint(show, m)`) and any generic `show(io, x)` over a Memory
# fell to the struct-style default and rendered as an empty `Memory{T}()`.
# Mirrors `show(io, ::Array)`; the multi-line "N-element Memory{T}:" form is the
# `display` representation and is produced separately.
function show(io::IO, m::Memory)
    _show_vector_compact(io, m)
end

# =============================================================================
# AbstractVector collection interface
# =============================================================================

function keys(m::Memory)
    return LinearIndices((length(m),))
end

function values(m::Memory)
    return m
end

function count(f::Function, m::Memory)
    n = 0
    for x in m
        if f(x)
            n = n + 1
        end
    end
    return n
end

# =============================================================================
# Constructors
# =============================================================================

function _memory_similar_dims(::Type{S}, dims::Tuple) where S
    if length(dims) == 1
        return Memory{S}(dims[1])
    end
    len = 1
    for d in dims
        if d < 0
            throw(DimensionMismatch("array dimensions must be non-negative"))
        end
        len = len * d
    end
    return wrap(Array, Memory{S}(len), dims)
end

function _memory_similar_vararg_dims(::Type{S}, d1::Int64, d2::Int64, dims::Tuple) where S
    if length(dims) == 0
        return _memory_similar_dims(S, (d1, d2))
    elseif length(dims) == 1
        return _memory_similar_dims(S, (d1, d2, dims[1]))
    elseif length(dims) == 2
        return _memory_similar_dims(S, (d1, d2, dims[1], dims[2]))
    elseif length(dims) == 3
        return _memory_similar_dims(S, (d1, d2, dims[1], dims[2], dims[3]))
    end
    throw(ArgumentError("similar(::Memory, dims...) supports up to 5 dimensions"))
end

function similar(m::Memory{T}) where T
    return Memory{T}(length(m))
end

function similar(m::Memory{T}, n::Int64) where T
    return Memory{T}(n)
end

function similar(m::Memory{T}, d1::Int64, d2::Int64, dims::Int64...) where T
    return _memory_similar_vararg_dims(T, d1, d2, dims)
end

function similar(m::Memory{T}, dims::Tuple) where T
    return _memory_similar_dims(T, dims)
end

function similar(m::Memory{T}, ::Type{S}) where {T,S}
    return Memory{S}(length(m))
end

function similar(m::Memory{T}, ::Type{S}, n::Int64) where {T,S}
    return Memory{S}(n)
end

function similar(m::Memory{T}, ::Type{S}, d1::Int64, d2::Int64, dims::Int64...) where {T,S}
    return _memory_similar_vararg_dims(S, d1, d2, dims)
end

function similar(m::Memory{T}, ::Type{S}, dims::Tuple) where {T,S}
    return _memory_similar_dims(S, dims)
end

# =============================================================================
# Copy
# =============================================================================

function unsafe_copyto!(dest::Memory{T}, doffs::Int64, src::Memory{T}, soffs::Int64, n::Int64) where T
    if n == 0
        return dest
    end
    if dest === src && doffs > soffs
        for i in n:-1:1
            dest[doffs + i - 1] = src[soffs + i - 1]
        end
    else
        for i in 1:n
            dest[doffs + i - 1] = src[soffs + i - 1]
        end
    end
    return dest
end

function unsafe_copyto!(dest::Memory, doffs::Int64, src::Memory, soffs::Int64, n::Int64)
    if n == 0
        return dest
    end
    if dest === src && doffs > soffs
        for i in n:-1:1
            dest[doffs + i - 1] = src[soffs + i - 1]
        end
    else
        for i in 1:n
            dest[doffs + i - 1] = src[soffs + i - 1]
        end
    end
    return dest
end

"""
    copy(m::Memory{T}) where T

Create a shallow copy of `m`.
"""
function copy(m::Memory{T}) where T
    n = length(m)
    result = Memory{T}(n)
    unsafe_copyto!(result, 1, m, 1, n)
    return result
end

function copyto!(dest::Memory, src::Memory)
    return copyto!(dest, 1, src, 1, length(src))
end

function copyto!(dest::Memory, doffs::Int64, src::Memory, soffs::Int64, n::Int64)
    if n < 0
        throw(ArgumentError("Number of elements to copy must be non-negative."))
    end
    unsafe_copyto!(dest, doffs, src, soffs, n)
    return dest
end
