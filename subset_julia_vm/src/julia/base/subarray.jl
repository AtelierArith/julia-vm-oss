# =============================================================================
# SubArray - Views of arrays without copying data
# =============================================================================
# Based on Julia's base/subarray.jl
#
# SubArray provides a lightweight view into a parent array.
# Modifications to the view affect the parent array directly.
#
# This implementation supports:
# - 1D array views with range indices (v = view(arr, 2:4))
# - Reading and writing through views
# - length, size, firstindex, lastindex operations
# - Multiple element types: Int64, Float64, Bool

# SubArray struct - a view into a parent array
# Uses parametric type T to support different element types
struct SubArray{T}
    parent::Vector{T}  # Reference to parent array
    offset::Int64      # Offset into parent (0-indexed internal offset)
    len::Int64         # Length of the view
end

# =============================================================================
# view function - Create a view of an array
# =============================================================================

# view(A, indices) creates a SubArray referencing A
# Implementations for different element types

function view(A::Vector{Float64}, indices::UnitRange)
    start_idx = first(indices)
    stop_idx = last(indices)

    # Bounds checking
    if start_idx < 1 || stop_idx > length(A)
        error("BoundsError: attempt to create view outside parent bounds")
    end

    offset = start_idx - 1  # Convert to 0-indexed offset
    len = stop_idx - start_idx + 1

    return SubArray{Float64}(A, offset, len)
end

function view(A::Vector{Int64}, indices::UnitRange)
    start_idx = first(indices)
    stop_idx = last(indices)

    # Bounds checking
    if start_idx < 1 || stop_idx > length(A)
        error("BoundsError: attempt to create view outside parent bounds")
    end

    offset = start_idx - 1  # Convert to 0-indexed offset
    len = stop_idx - start_idx + 1

    return SubArray{Int64}(A, offset, len)
end

function view(A::Vector{Bool}, indices::UnitRange)
    start_idx = first(indices)
    stop_idx = last(indices)

    # Bounds checking
    if start_idx < 1 || stop_idx > length(A)
        error("BoundsError: attempt to create view outside parent bounds")
    end

    offset = start_idx - 1  # Convert to 0-indexed offset
    len = stop_idx - start_idx + 1

    return SubArray{Bool}(A, offset, len)
end

# Single integer index - returns the element directly (no SubArray)
function view(A::Vector{Float64}, i::Int64)
    return A[i]
end

function view(A::Vector{Int64}, i::Int64)
    return A[i]
end

function view(A::Vector{Bool}, i::Int64)
    return A[i]
end

# =============================================================================
# Array interface for SubArray - works for all element types
# =============================================================================

# length returns the number of elements in the view
function length(v::SubArray{Float64})
    return v.len
end

function length(v::SubArray{Int64})
    return v.len
end

function length(v::SubArray{Bool})
    return v.len
end

# size returns the shape of the view
function size(v::SubArray{Float64})
    return (v.len,)
end

function size(v::SubArray{Int64})
    return (v.len,)
end

function size(v::SubArray{Bool})
    return (v.len,)
end

function size(v::SubArray{Float64}, dim::Int64)
    if dim == 1
        return v.len
    else
        return 1
    end
end

function size(v::SubArray{Int64}, dim::Int64)
    if dim == 1
        return v.len
    else
        return 1
    end
end

function size(v::SubArray{Bool}, dim::Int64)
    if dim == 1
        return v.len
    else
        return 1
    end
end

# firstindex/lastindex for SubArray
function firstindex(v::SubArray{Float64})
    return 1
end

function firstindex(v::SubArray{Int64})
    return 1
end

function firstindex(v::SubArray{Bool})
    return 1
end

function lastindex(v::SubArray{Float64})
    return v.len
end

function lastindex(v::SubArray{Int64})
    return v.len
end

function lastindex(v::SubArray{Bool})
    return v.len
end

# ndims for SubArray
function ndims(v::SubArray{Float64})
    return 1
end

function ndims(v::SubArray{Int64})
    return 1
end

function ndims(v::SubArray{Bool})
    return 1
end

# =============================================================================
# Indexing operations for SubArray
# =============================================================================

# getindex: v[i] returns the element at position i in the view
function getindex(v::SubArray{Float64}, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access SubArray at index")
    end
    parent_idx = v.offset + i
    return v.parent[parent_idx]
end

function getindex(v::SubArray{Int64}, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access SubArray at index")
    end
    parent_idx = v.offset + i
    return v.parent[parent_idx]
end

function getindex(v::SubArray{Bool}, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access SubArray at index")
    end
    parent_idx = v.offset + i
    return v.parent[parent_idx]
end

# setindex!: v[i] = x sets the element at position i in the view
function setindex!(v::SubArray{Float64}, x, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access SubArray at index")
    end
    parent_idx = v.offset + i
    setindex!(v.parent, x, parent_idx)
    return x
end

function setindex!(v::SubArray{Int64}, x, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access SubArray at index")
    end
    parent_idx = v.offset + i
    setindex!(v.parent, x, parent_idx)
    return x
end

function setindex!(v::SubArray{Bool}, x, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access SubArray at index")
    end
    parent_idx = v.offset + i
    setindex!(v.parent, x, parent_idx)
    return x
end

# =============================================================================
# Conversion functions
# =============================================================================

# collect: Convert SubArray to a regular Array (makes a copy)
function collect(v::SubArray{Float64})
    n = v.len
    result = zeros(n)
    for i in 1:n
        result[i] = getindex(v, i)
    end
    return result
end

function collect(v::SubArray{Int64})
    n = v.len
    result = zeros(Int64, n)
    for i in 1:n
        result[i] = getindex(v, i)
    end
    return result
end

function collect(v::SubArray{Bool})
    n = v.len
    result = fill(false, n)
    for i in 1:n
        result[i] = getindex(v, i)
    end
    return result
end

# parent: Return the parent array of a SubArray
function parent(v::SubArray{Float64})
    return v.parent
end

function parent(v::SubArray{Int64})
    return v.parent
end

function parent(v::SubArray{Bool})
    return v.parent
end

# parentindices: Return the indices into the parent array
function parentindices(v::SubArray{Float64})
    start_idx = v.offset + 1
    stop_idx = v.offset + v.len
    return (start_idx:stop_idx,)
end

function parentindices(v::SubArray{Int64})
    start_idx = v.offset + 1
    stop_idx = v.offset + v.len
    return (start_idx:stop_idx,)
end

function parentindices(v::SubArray{Bool})
    start_idx = v.offset + 1
    stop_idx = v.offset + v.len
    return (start_idx:stop_idx,)
end

# =============================================================================
# @view macro - Transform A[i:j] to view(A, i:j)
# =============================================================================
# This macro is defined in macros.jl

# =============================================================================
# @views macro - Transform all indexing in a block to views
# =============================================================================
# This macro is defined in macros.jl
