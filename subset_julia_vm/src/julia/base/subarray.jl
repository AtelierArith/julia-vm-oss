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
# - Multiple element types: Int64, Float64, Bool, Int8

# SubArray struct - a view into a parent array
# Mirrors the upstream parameter surface for 1D representable views:
#   SubArray{T,N,P,I,L} <: AbstractArray{T,N}
# while retaining the subset VM's compact offset/length storage.
struct SubArray{T,N,P,I,L} <: AbstractArray{T,N}
    parent::P      # Reference to parent array
    indices::I     # Parent indices tuple, e.g. (2:4,)
    offset::Int64  # Offset into parent (0-indexed internal offset)
    len::Int64     # Length of the view
end

struct Slice{T}
    indices::T
end

function Slice(indices)
    return Slice{typeof(indices)}(indices)
end

function length(s::Slice{T}) where {T}
    return length(s.indices)
end

function first(s::Slice{T}) where {T}
    return first(s.indices)
end

function last(s::Slice{T}) where {T}
    return last(s.indices)
end

function getindex(s::Slice{T}, i::Int64) where {T}
    return s.indices[i]
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

    return SubArray{Float64,1,Vector{Float64},Tuple{UnitRange{Int64}},true}(A, (indices,), offset, len)
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

    return SubArray{Int64,1,Vector{Int64},Tuple{UnitRange{Int64}},true}(A, (indices,), offset, len)
end

function view(A::Vector{Int8}, indices::UnitRange)
    start_idx = first(indices)
    stop_idx = last(indices)

    if start_idx < 1 || stop_idx > length(A)
        error("BoundsError: attempt to create view outside parent bounds")
    end

    offset = start_idx - 1
    len = stop_idx - start_idx + 1

    return SubArray{Int8,1,Vector{Int8},Tuple{UnitRange{Int64}},true}(A, (indices,), offset, len)
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

    return SubArray{Bool,1,Vector{Bool},Tuple{UnitRange{Int64}},true}(A, (indices,), offset, len)
end

function view(A::Vector{T}, indices::UnitRange) where T
    start_idx = first(indices)
    stop_idx = last(indices)

    if start_idx < 1 || stop_idx > length(A)
        error("BoundsError: attempt to create view outside parent bounds")
    end

    offset = start_idx - 1
    len = stop_idx - start_idx + 1

    return SubArray{T,1,Vector{T},Tuple{UnitRange{Int64}},true}(A, (indices,), offset, len)
end

function view(A::Vector{T}, indices::Vector{Int64}) where T
    len = length(indices)
    for i in 1:len
        idx = indices[i]
        if idx < 1 || idx > length(A)
            error("BoundsError: attempt to create view outside parent bounds")
        end
    end

    return SubArray{T,1,Vector{T},Tuple{Vector{Int64}},false}(A, (indices,), 0, len)
end

function view(r::AbstractRange, indices::UnitRange)
    return r[indices]
end

function view(r::OneTo, indices::UnitRange)
    return r[indices]
end

# Effective result rank of a SubArray. A two-index view is genuinely 2-D only
# when neither stored index is a scalar Int; a scalar index drops its
# dimension (Issue #5137), so view(A, i, :) / view(A, :, j) is 1-D even though
# `v.indices` still holds two entries.
function _subarray_ndims(v)
    n = length(v.indices)
    if n == 2
        drop = 0
        if v.indices[1] isa Int64
            drop = drop + 1
        end
        if v.indices[2] isa Int64
            drop = drop + 1
        end
        return n - drop
    end
    return n
end

function _matrix_range_view_len(A, rows, cols)
    first_row = first(rows)
    last_row = last(rows)
    first_col = first(cols)
    last_col = last(cols)

    if first_row < 1 || last_row > size(A, 1) || first_col < 1 || last_col > size(A, 2)
        error("BoundsError: attempt to create view outside parent bounds")
    end

    return length(rows) * length(cols)
end

function view(A::Matrix{T}, rows::UnitRange, cols::UnitRange) where T
    len = _matrix_range_view_len(A, rows, cols)
    return SubArray{T,2,Matrix{T},Tuple{UnitRange{Int64},UnitRange{Int64}},false}(A, (rows, cols), 0, len)
end

function view(A::Matrix{T}, rows::UnitRange, cols::typeof(:)) where T
    col_indices = Slice(1:size(A, 2))
    len = _matrix_range_view_len(A, rows, col_indices)
    return SubArray{T,2,Matrix{T},Tuple{UnitRange{Int64},Slice{UnitRange{Int64}}},false}(A, (rows, col_indices), 0, len)
end

function view(A::Matrix{T}, rows::typeof(:), cols::UnitRange) where T
    row_indices = Slice(1:size(A, 1))
    len = _matrix_range_view_len(A, row_indices, cols)
    return SubArray{T,2,Matrix{T},Tuple{Slice{UnitRange{Int64}},UnitRange{Int64}},true}(A, (row_indices, cols), 0, len)
end

function view(A::Matrix{T}, rows::typeof(:), cols::typeof(:)) where T
    row_indices = Slice(1:size(A, 1))
    col_indices = Slice(1:size(A, 2))
    len = _matrix_range_view_len(A, row_indices, col_indices)
    return SubArray{T,2,Matrix{T},Tuple{Slice{UnitRange{Int64}},Slice{UnitRange{Int64}}},true}(A, (row_indices, col_indices), 0, len)
end

# Dimension-dropping matrix views: a scalar Int index keeps the parent
# dimension fixed and drops it from the result, so view(A, i, idx) /
# view(A, idx, j) is a 1-D SubArray (a row or column slice) — mirroring
# upstream Julia's index_dimsum. The stored indices tuple still has two
# entries (one of them the scalar), which the shared column-major
# `_subarray_linear_load`/`_subarray_linear_store!` map correctly because a
# scalar index has length 1 and `s[1] == s` (Issue #5137).
function view(A::Matrix{T}, row::Int64, cols::UnitRange) where T
    if row < 1 || row > size(A, 1) || first(cols) < 1 || last(cols) > size(A, 2)
        error("BoundsError: attempt to create view outside parent bounds")
    end
    return SubArray{T,1,Matrix{T},Tuple{Int64,UnitRange{Int64}},true}(A, (row, cols), 0, length(cols))
end

function view(A::Matrix{T}, row::Int64, cols::typeof(:)) where T
    if row < 1 || row > size(A, 1)
        error("BoundsError: attempt to create view outside parent bounds")
    end
    col_indices = Slice(1:size(A, 2))
    return SubArray{T,1,Matrix{T},Tuple{Int64,Slice{UnitRange{Int64}}},true}(A, (row, col_indices), 0, size(A, 2))
end

function view(A::Matrix{T}, rows::UnitRange, col::Int64) where T
    if col < 1 || col > size(A, 2) || first(rows) < 1 || last(rows) > size(A, 1)
        error("BoundsError: attempt to create view outside parent bounds")
    end
    return SubArray{T,1,Matrix{T},Tuple{UnitRange{Int64},Int64},true}(A, (rows, col), 0, length(rows))
end

function view(A::Matrix{T}, rows::typeof(:), col::Int64) where T
    if col < 1 || col > size(A, 2)
        error("BoundsError: attempt to create view outside parent bounds")
    end
    row_indices = Slice(1:size(A, 1))
    return SubArray{T,1,Matrix{T},Tuple{Slice{UnitRange{Int64}},Int64},true}(A, (row_indices, col), 0, size(A, 1))
end

# 3-D range view (Issue #5137). The shared column-major
# `_subarray_linear_load`/`_subarray_linear_store!` map a 3-index `v.indices`
# correctly, so this constructs the SubArray and records the element count.
function view(A::Array{T,3}, d1::UnitRange, d2::UnitRange, d3::UnitRange) where T
    if first(d1) < 1 || last(d1) > size(A, 1) ||
       first(d2) < 1 || last(d2) > size(A, 2) ||
       first(d3) < 1 || last(d3) > size(A, 3)
        error("BoundsError: attempt to create view outside parent bounds")
    end
    len = length(d1) * length(d2) * length(d3)
    return SubArray{T,3,typeof(A),Tuple{UnitRange{Int64},UnitRange{Int64},UnitRange{Int64}},false}(
        A, (d1, d2, d3), 0, len)
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

function view(A::Vector{T}, i::Int64) where T
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

function length(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return v.len
end

# size returns the shape of the view
function size(v::SubArray{Float64,1,P,I,L}) where {P,I,L}
    return (v.len,)
end

function size(v::SubArray{Int64,1,P,I,L}) where {P,I,L}
    return (v.len,)
end

function size(v::SubArray{Bool,1,P,I,L}) where {P,I,L}
    return (v.len,)
end

function size(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    nd = _subarray_ndims(v)
    if nd == 2
        return (length(v.indices[1]), length(v.indices[2]))
    elseif nd == 3
        return (length(v.indices[1]), length(v.indices[2]), length(v.indices[3]))
    end
    return (v.len,)
end

function size(v::SubArray{T,2,P,I,L}) where {T,P,I,L}
    return (length(v.indices[1]), length(v.indices[2]))
end

function size(v::SubArray{T,3,P,I,L}) where {T,P,I,L}
    return (length(v.indices[1]), length(v.indices[2]), length(v.indices[3]))
end

function size(v::SubArray{Float64,1,P,I,L}, dim::Int64) where {P,I,L}
    if dim == 1
        return v.len
    else
        return 1
    end
end

function size(v::SubArray{Int64,1,P,I,L}, dim::Int64) where {P,I,L}
    if dim == 1
        return v.len
    else
        return 1
    end
end

function size(v::SubArray{Bool,1,P,I,L}, dim::Int64) where {P,I,L}
    if dim == 1
        return v.len
    else
        return 1
    end
end

function size(v::SubArray{T,N,P,I,L}, dim::Int64) where {T,N,P,I,L}
    nd = _subarray_ndims(v)
    if (nd == 2 || nd == 3) && dim >= 1 && dim <= nd
        return length(v.indices[dim])
    end
    if nd == 2 || nd == 3
        return 1
    end
    if dim == 1
        return v.len
    else
        return 1
    end
end

function size(v::SubArray{T,2,P,I,L}, dim::Int64) where {T,P,I,L}
    if dim == 1
        return length(v.indices[1])
    elseif dim == 2
        return length(v.indices[2])
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

function firstindex(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
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

function lastindex(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
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

function ndims(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return _subarray_ndims(v)
end

function eltype(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return T
end

function IteratorSize(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return HasLength()
end

# `map` over a SubArray materializes it first (the runtime `collect` iterator
# path does not handle the SubArray struct, but `collect(::SubArray)` does), so
# `map(f, view(v, a:b))` matches upstream (Issue #5137).
function map(f, v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return map(f, collect(v))
end

function IteratorEltype(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return HasEltype()
end

function iterate(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    if v.len == 0
        return nothing
    end
    return (getindex(v, 1), Int64(2))
end

function iterate(v::SubArray{T,N,P,I,L}, state::Int64) where {T,N,P,I,L}
    if state > v.len
        return nothing
    end
    return (getindex(v, state), state + Int64(1))
end

# =============================================================================
# Indexing operations for SubArray
# =============================================================================

# Linear (single-index) read of a view. A 1D view is contiguous in the parent, so
# `offset + i` suffices. A 2D view is NOT contiguous: the column-major linear index
# must be split into a `(row, col)` pair within the view and routed through the
# per-dimension parent indices, otherwise `v[i]` returns the parent's `i`-th linear
# element instead of the view's (Issue #5816).
function _subarray_linear_load(v, i)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access SubArray at index")
    end
    nd = length(v.indices)
    if nd == 1
        return v.parent[v.indices[1][i]]
    elseif nd == 2
        # Column-major linear -> (row, col), then index the parent through the
        # per-dimension indices (the same mapping the 2D Cartesian getindex uses).
        nrows = length(v.indices[1])
        row = mod(i - 1, nrows) + 1
        col = div(i - 1, nrows) + 1
        return v.parent[v.indices[1][row], v.indices[2][col]]
    else
        # 3-D column-major: split the linear index into (row, col, page) and
        # route each through the per-dimension parent indices (Issue #5137).
        n1 = length(v.indices[1])
        n2 = length(v.indices[2])
        n12 = n1 * n2
        page = div(i - 1, n12) + 1
        rem1 = mod(i - 1, n12)
        row = mod(rem1, n1) + 1
        col = div(rem1, n1) + 1
        return v.parent[v.indices[1][row], v.indices[2][col], v.indices[3][page]]
    end
end

# getindex: v[i] returns the element at position i in the view
function getindex(v::SubArray{Float64}, i::Int64)
    return _subarray_linear_load(v, i)
end

function getindex(v::SubArray{Int64}, i::Int64)
    return _subarray_linear_load(v, i)
end

function getindex(v::SubArray{Bool}, i::Int64)
    return _subarray_linear_load(v, i)
end

function getindex(v::SubArray{T,N,P,I,L}, i::Int64) where {T,N,P,I,L}
    return _subarray_linear_load(v, i)
end

function getindex(v::SubArray{T,2,P,I,L}, i::Int64, j::Int64) where {T,P,I,L}
    if i < 1 || i > size(v, 1) || j < 1 || j > size(v, 2)
        error("BoundsError: attempt to access SubArray at index")
    end
    parent_i = v.indices[1][i]
    parent_j = v.indices[2][j]
    return v.parent[parent_i, parent_j]
end

function getindex(v::SubArray{T,3,P,I,L}, i::Int64, j::Int64, k::Int64) where {T,P,I,L}
    if i < 1 || i > size(v, 1) || j < 1 || j > size(v, 2) || k < 1 || k > size(v, 3)
        error("BoundsError: attempt to access SubArray at index")
    end
    return v.parent[v.indices[1][i], v.indices[2][j], v.indices[3][k]]
end

# setindex!: v[i] = x sets the element at position i in the view.
# Shared linear store: a 1D view is contiguous in the parent (`offset + i`), but a
# 2D view is NOT — the column-major linear index must map to `(row, col)` and then
# through the per-dimension parent indices, exactly mirroring `_subarray_linear_load`
# (Issue #5816). Returns the view `v` (Julia `setindex!` returns the collection) so
# the compiler's post-IndexStore StoreBack writes the view back unchanged.
function _subarray_linear_store!(v, x, i)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access SubArray at index")
    end
    nd = length(v.indices)
    if nd == 1
        setindex!(v.parent, x, v.indices[1][i])
    elseif nd == 2
        nrows = length(v.indices[1])
        row = mod(i - 1, nrows) + 1
        col = div(i - 1, nrows) + 1
        setindex!(v.parent, x, v.indices[1][row], v.indices[2][col])
    else
        n1 = length(v.indices[1])
        n2 = length(v.indices[2])
        n12 = n1 * n2
        page = div(i - 1, n12) + 1
        rem1 = mod(i - 1, n12)
        row = mod(rem1, n1) + 1
        col = div(rem1, n1) + 1
        setindex!(v.parent, x, v.indices[1][row], v.indices[2][col], v.indices[3][page])
    end
    return v
end

# 3-D Cartesian setindex! writes through to the parent (Issue #5137).
function setindex!(v::SubArray{T,3,P,I,L}, x, i::Int64, j::Int64, k::Int64) where {T,P,I,L}
    if i < 1 || i > size(v, 1) || j < 1 || j > size(v, 2) || k < 1 || k > size(v, 3)
        error("BoundsError: attempt to access SubArray at index")
    end
    setindex!(v.parent, x, v.indices[1][i], v.indices[2][j], v.indices[3][k])
    return v
end

function setindex!(v::SubArray{Float64}, x, i::Int64)
    return _subarray_linear_store!(v, x, i)
end

function setindex!(v::SubArray{Int64}, x, i::Int64)
    return _subarray_linear_store!(v, x, i)
end

function setindex!(v::SubArray{Bool}, x, i::Int64)
    return _subarray_linear_store!(v, x, i)
end

function setindex!(v::SubArray{T,N,P,I,L}, x, i::Int64) where {T,N,P,I,L}
    return _subarray_linear_store!(v, x, i)
end

function setindex!(v::SubArray{T,2,P,I,L}, x, i::Int64, j::Int64) where {T,P,I,L}
    if i < 1 || i > size(v, 1) || j < 1 || j > size(v, 2)
        error("BoundsError: attempt to access SubArray at index")
    end
    parent_i = v.indices[1][i]
    parent_j = v.indices[2][j]
    setindex!(v.parent, x, parent_i, parent_j)
    return v
end

# =============================================================================
# Conversion functions
# =============================================================================

function _collect_subarray(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    if _subarray_ndims(v) == 2
        return _collect_subarray_2d(v)
    end
    if length(v.indices) == 3
        return _collect_subarray_3d(v)
    end

    n = v.len
    result = _array_undef_from_dims(T, (n,))
    for i in 1:n
        result[i] = getindex(v, i)
    end
    return result
end

function _collect_subarray_3d(v::SubArray{T,3,P,I,L}) where {T,P,I,L}
    d1 = length(v.indices[1])
    d2 = length(v.indices[2])
    d3 = length(v.indices[3])
    result = _array_undef_from_dims(T, (d1, d2, d3))
    for k in 1:d3
        for j in 1:d2
            for i in 1:d1
                result[i, j, k] = getindex(v, i, j, k)
            end
        end
    end
    return result
end

function _collect_subarray_2d(v::SubArray{T,2,P,I,L}) where {T,P,I,L}
    rows = length(v.indices[1])
    cols = length(v.indices[2])
    result = _array_undef_from_dims(T, (rows, cols))
    for j in 1:cols
        for i in 1:rows
            result[i, j] = getindex(v, i, j)
        end
    end
    return result
end

# collect: Convert SubArray to a regular Array (makes a copy)
function collect(v::SubArray{Float64,2,P,I,L}) where {P,I,L}
    return _collect_subarray_2d(v)
end

function collect(v::SubArray{Int64,2,P,I,L}) where {P,I,L}
    return _collect_subarray_2d(v)
end

function collect(v::SubArray{Bool,2,P,I,L}) where {P,I,L}
    return _collect_subarray_2d(v)
end

function collect(v::SubArray{T,2,P,I,L}) where {T,P,I,L}
    return _collect_subarray_2d(v)
end

function collect(v::SubArray{Float64})
    return _collect_subarray(v)
end

function collect(v::SubArray{Int64})
    return _collect_subarray(v)
end

function collect(v::SubArray{Bool})
    return _collect_subarray(v)
end

function collect(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return _collect_subarray(v)
end

# =============================================================================
# similar / copy / zero for SubArray (Issue #8003)
# =============================================================================
#
# A SubArray is an AbstractArray, so upstream's generic fallbacks make
# `similar`/`copy`/`zero` of a view return a fresh dense `Array` of the right
# eltype/shape:
#   similar(::AbstractArray, ::Type{T}, dims) = Array{T,N}(undef, dims)
#   copy(::AbstractArray)                     materialises the viewed elements
#   zero(::AbstractArray{T})                  fills a similar array with zero(T)
# The subset VM only defined these for `Array`/`Memory`, so `similar(view(...))`
# fell through to the Rust builtin and errored ("similar requires an array or
# memory argument"), and `copy` (which calls `similar`) failed the same way.
# Mirror the AbstractArray contract here so a view copies/zeros like any other
# array — this unblocks generic `AbstractVector` state in the ODE stepper (#7986).

function similar(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return _array_undef_from_dims(eltype(v), size(v))
end

function similar(v::SubArray{T,N,P,I,L}, ::Type{S}) where {T,N,P,I,L,S}
    return _array_undef_from_dims(S, size(v))
end

function similar(v::SubArray{T,N,P,I,L}, dims::Tuple) where {T,N,P,I,L}
    return _array_undef_from_dims(eltype(v), dims)
end

function similar(v::SubArray{T,N,P,I,L}, ::Type{S}, dims::Tuple) where {T,N,P,I,L,S}
    return _array_undef_from_dims(S, dims)
end

function similar(v::SubArray{T,N,P,I,L}, dims::Int64...) where {T,N,P,I,L}
    return _array_undef_from_dims(eltype(v), dims)
end

function similar(v::SubArray{T,N,P,I,L}, ::Type{S}, dims::Int64...) where {T,N,P,I,L,S}
    return _array_undef_from_dims(S, dims)
end

# copy materialises a view's elements into a fresh, independent Array.
function copy(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return _collect_subarray(v)
end

# zero returns a zero-filled Array with the view's eltype and shape.
function zero(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return zeros(eltype(v), size(v))
end

# =============================================================================
# Display for SubArray (Issue #8003)
# =============================================================================
#
# A SubArray is an AbstractArray and upstream displays it element-wise, not as
# its raw internal struct. Without a `show(io, ::SubArray)` method the VM's
# print path fell back to the generic struct-field dump
# (`SubArray{...}([1.0, ...], (1:3,), 0, 3)`). Route through the materialised
# Array's `show`, which already renders the compact `[a, b, c]` (1-D) and
# `[a b; c d]` (2-D) forms with the correct eltype prefix and per-element show.
function show(io::IO, v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    show(io, collect(v))
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

function parent(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
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

function parentindices(v::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return v.indices
end

# =============================================================================
# ReshapedArray - reshape wrapper for non-Array parents (Issue #5137)
# =============================================================================
#
# Julia keeps `reshape(::Array, ...)` as an Array with shared storage, but wraps
# non-Array parents such as ranges and views in `Base.ReshapedArray{T,N,P,MI}`.
# This focused subset supports range/SubArray parents and delegates indexing
# back to the 1D parent in column-major order.

struct ReshapedArray{T,N,P,MI} <: AbstractArray{T,N}
    parent::P
    dims::Tuple
end

function _reshapedarray_checked(a::P, dims::Tuple) where P
    len = _array_length_from_size(dims)
    if len != length(a)
        throw(DimensionMismatch("new dimensions must be consistent with array length"))
    end

    T = eltype(a)
    n = length(dims)
    if n == 1
        return ReshapedArray{T,1,P,Tuple{}}(a, dims)
    elseif n == 2
        return ReshapedArray{T,2,P,Tuple{}}(a, dims)
    elseif n == 3
        return ReshapedArray{T,3,P,Tuple{}}(a, dims)
    elseif n == 4
        return ReshapedArray{T,4,P,Tuple{}}(a, dims)
    else
        error("ReshapedArray supports up to 4 dimensions")
    end
end

function reshape(a::SubArray{T,N,P,I,L}, dims::Tuple) where {T,N,P,I,L}
    return _reshapedarray_checked(a, dims)
end

function reshape(a::SubArray{T,N,P,I,L}, dims::Int64...) where {T,N,P,I,L}
    return _reshapedarray_checked(a, dims)
end

function length(a::ReshapedArray{T,N,P,MI}) where {T,N,P,MI}
    return _array_length_from_size(a.dims)
end

function size(a::ReshapedArray{T,N,P,MI}) where {T,N,P,MI}
    return a.dims
end

function size(a::ReshapedArray{T,N,P,MI}, dim::Int64) where {T,N,P,MI}
    if dim > length(a.dims)
        return 1
    end
    return a.dims[dim]
end

function ndims(a::ReshapedArray{T,N,P,MI}) where {T,N,P,MI}
    return N
end

function eltype(a::ReshapedArray{T,N,P,MI}) where {T,N,P,MI}
    return T
end

function parent(a::ReshapedArray{T,N,P,MI}) where {T,N,P,MI}
    return a.parent
end

function parentindices(a::ReshapedArray{T,N,P,MI}) where {T,N,P,MI}
    return (1:length(a.parent),)
end

function _reshapedarray_linear_index(dims::Tuple, I::Tuple)
    if length(I) != length(dims)
        error("BoundsError: invalid number of indices")
    end
    linear = I[1]
    stride = 1
    for dim in 2:length(I)
        stride = stride * dims[dim - 1]
        linear = linear + (I[dim] - 1) * stride
    end
    return linear
end

function getindex(a::ReshapedArray{T,N,P,MI}, i::Int64) where {T,N,P,MI}
    if i < 1 || i > length(a)
        error("BoundsError: attempt to access ReshapedArray at index")
    end
    return a.parent[i]
end

function getindex(a::ReshapedArray{T,N,P,MI}, I::Int64...) where {T,N,P,MI}
    linear = _reshapedarray_linear_index(a.dims, I)
    if linear < 1 || linear > length(a)
        error("BoundsError: attempt to access ReshapedArray at index")
    end
    return a.parent[linear]
end

function setindex!(a::ReshapedArray{T,N,P,MI}, x, i::Int64) where {T,N,P,MI}
    if i < 1 || i > length(a)
        error("BoundsError: attempt to access ReshapedArray at index")
    end
    setindex!(a.parent, x, i)
    return a
end

function setindex!(a::ReshapedArray{T,N,P,MI}, x, I::Int64...) where {T,N,P,MI}
    linear = _reshapedarray_linear_index(a.dims, I)
    if linear < 1 || linear > length(a)
        error("BoundsError: attempt to access ReshapedArray at index")
    end
    setindex!(a.parent, x, linear)
    return a
end

# Linear iteration / collect / map over a 1-D ReshapedArray (via its element
# getindex, which delegates to the parent — including a SubArray parent), so
# `collect`, `map`, and `sum` over `reshape(view(...), n)` work (Issue #5137).
# The runtime `collect` iterator path does not recognize the ReshapedArray
# struct. Restricted to the 1-D shape, which is the common `reshape(view, n)`
# case; multi-dim reshaped views are left to the existing path.
function map(f, a::ReshapedArray{T,1,P,MI}) where {T,P,MI}
    return map(f, collect(a))
end

function iterate(a::ReshapedArray{T,1,P,MI}) where {T,P,MI}
    if length(a) == 0
        return nothing
    end
    return (getindex(a, 1), Int64(2))
end

function iterate(a::ReshapedArray{T,1,P,MI}, state::Int64) where {T,P,MI}
    if state > length(a)
        return nothing
    end
    return (getindex(a, state), state + Int64(1))
end

function collect(a::ReshapedArray{T,1,P,MI}) where {T,P,MI}
    n = length(a)
    result = _array_undef_from_dims(T, (n,))
    for i in 1:n
        result[i] = getindex(a, i)
    end
    return result
end

# =============================================================================
# MatrixView - 1D view of a row or column of a 2D matrix (Issues #3593, #3594)
# =============================================================================
# A lightweight view that proxies getindex/setindex! through to a parent Matrix
# along one fixed dimension. Used to back `selectdim(A::Matrix, d, i)` and
# `dropdims(A::Matrix; dims)` so that mutations alias the parent storage and
# the element type is preserved.
#
# Field semantics:
#   parent       - the source 2D matrix (storage is shared by reference)
#   fixed_dim    - 1 = fixed row index (view is a row),
#                  2 = fixed column index (view is a column)
#   fixed_index  - which row/column is fixed (1-based)
#   len          - number of elements in the resulting 1D view
#
# Note: parent is intentionally untyped because Matrix{T} is not currently
# usable as a struct field type for dispatch in SubsetJuliaVM. The element
# type T is carried by the parametric SubArray-style {T} parameter, which
# is what method dispatch looks at.
struct MatrixView{T}
    parent
    fixed_dim::Int64
    fixed_index::Int64
    len::Int64
end

# -----------------------------------------------------------------------------
# Indexing (getindex / setindex!)
# -----------------------------------------------------------------------------
# Note: setindex! returns the view `v` rather than `x`. SubsetJuliaVM's
# IndexAssign lowering (compile/stmt.rs) emits StoreArray after IndexStore,
# which stores the setindex! return value back into the variable bound to
# the view. Returning `v` keeps the variable's binding as a MatrixView so
# subsequent `v[i]` reads continue to work. Existing 1D SubArray methods
# work via a different path because the inner builtin setindex! on the
# parent Array bubbles the array reference up.

function getindex(v::MatrixView{Float64}, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access MatrixView at index $i")
    end
    p = v.parent
    fd = v.fixed_dim
    fi = v.fixed_index
    if fd == 1
        return p[fi, i]
    else
        return p[i, fi]
    end
end

function getindex(v::MatrixView{Int64}, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access MatrixView at index $i")
    end
    p = v.parent
    fd = v.fixed_dim
    fi = v.fixed_index
    if fd == 1
        return p[fi, i]
    else
        return p[i, fi]
    end
end

function getindex(v::MatrixView{Bool}, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access MatrixView at index $i")
    end
    p = v.parent
    fd = v.fixed_dim
    fi = v.fixed_index
    if fd == 1
        return p[fi, i]
    else
        return p[i, fi]
    end
end

function setindex!(v::MatrixView{Float64}, x, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access MatrixView at index $i")
    end
    p = v.parent
    fd = v.fixed_dim
    fi = v.fixed_index
    if fd == 1
        p[fi, i] = x
    else
        p[i, fi] = x
    end
    return v
end

function setindex!(v::MatrixView{Int64}, x, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access MatrixView at index $i")
    end
    p = v.parent
    fd = v.fixed_dim
    fi = v.fixed_index
    if fd == 1
        p[fi, i] = x
    else
        p[i, fi] = x
    end
    return v
end

function setindex!(v::MatrixView{Bool}, x, i::Int64)
    if i < 1 || i > v.len
        error("BoundsError: attempt to access MatrixView at index $i")
    end
    p = v.parent
    fd = v.fixed_dim
    fi = v.fixed_index
    if fd == 1
        p[fi, i] = x
    else
        p[i, fi] = x
    end
    return v
end

# -----------------------------------------------------------------------------
# Array interface for MatrixView
# -----------------------------------------------------------------------------

function length(v::MatrixView{Float64})
    return v.len
end

function length(v::MatrixView{Int64})
    return v.len
end

function length(v::MatrixView{Bool})
    return v.len
end

function size(v::MatrixView{Float64})
    return (v.len,)
end

function size(v::MatrixView{Int64})
    return (v.len,)
end

function size(v::MatrixView{Bool})
    return (v.len,)
end

function size(v::MatrixView{Float64}, dim::Int64)
    if dim == 1
        return v.len
    else
        return 1
    end
end

function size(v::MatrixView{Int64}, dim::Int64)
    if dim == 1
        return v.len
    else
        return 1
    end
end

function size(v::MatrixView{Bool}, dim::Int64)
    if dim == 1
        return v.len
    else
        return 1
    end
end

function ndims(v::MatrixView{Float64})
    return 1
end

function ndims(v::MatrixView{Int64})
    return 1
end

function ndims(v::MatrixView{Bool})
    return 1
end

function firstindex(v::MatrixView{Float64})
    return 1
end

function firstindex(v::MatrixView{Int64})
    return 1
end

function firstindex(v::MatrixView{Bool})
    return 1
end

function lastindex(v::MatrixView{Float64})
    return v.len
end

function lastindex(v::MatrixView{Int64})
    return v.len
end

function lastindex(v::MatrixView{Bool})
    return v.len
end

function parent(v::MatrixView{Float64})
    return v.parent
end

function parent(v::MatrixView{Int64})
    return v.parent
end

function parent(v::MatrixView{Bool})
    return v.parent
end

# -----------------------------------------------------------------------------
# selectdim and dropdims overrides backed by MatrixView (Issues #3593, #3594)
# -----------------------------------------------------------------------------
# Replace the array.jl fallbacks (which copy via zeros(...)) with view-based
# implementations that share parent storage and preserve element type.
# Kept in subarray.jl because that's where MatrixView lives, and it loads
# after array.jl so these methods override the earlier definitions.

function _matrix_view_for(A, fixed_dim::Int64, fixed_index::Int64, len::Int64)
    et = eltype(A)
    if et == Float64
        return MatrixView{Float64}(A, fixed_dim, fixed_index, len)
    elseif et == Int64
        return MatrixView{Int64}(A, fixed_dim, fixed_index, len)
    elseif et == Bool
        return MatrixView{Bool}(A, fixed_dim, fixed_index, len)
    else
        return nothing
    end
end

function selectdim(A, d, i)
    if ndims(A) != 2
        error("selectdim: only 2D matrices are supported")
    end
    m = size(A, 1)
    n = size(A, 2)
    if d == 1
        if i < 1 || i > m
            error("BoundsError: selectdim index $i out of range 1:$m for dimension 1")
        end
        view = _matrix_view_for(A, 1, i, n)
        if view !== nothing
            return view
        end
        # Fallback: copy for eltypes without a MatrixView specialization.
        result = similar(A, n)
        for j in 1:n
            result[j] = A[i, j]
        end
        return result
    elseif d == 2
        if i < 1 || i > n
            error("BoundsError: selectdim index $i out of range 1:$n for dimension 2")
        end
        view = _matrix_view_for(A, 2, i, m)
        if view !== nothing
            return view
        end
        result = similar(A, m)
        for k in 1:m
            result[k] = A[k, i]
        end
        return result
    else
        error("selectdim: dimension must be 1 or 2 for matrices")
    end
end

function dropdims(A; dims)
    if ndims(A) != 2
        error("dropdims: only 2D matrices are supported")
    end
    m = size(A, 1)
    n = size(A, 2)
    if dims == 1
        if m != 1
            error("dropdims: dimension 1 has size $m, must be 1")
        end
        view = _matrix_view_for(A, 1, 1, n)
        if view !== nothing
            return view
        end
        result = similar(A, n)
        for j in 1:n
            result[j] = A[1, j]
        end
        return result
    elseif dims == 2
        if n != 1
            error("dropdims: dimension 2 has size $n, must be 1")
        end
        view = _matrix_view_for(A, 2, 1, m)
        if view !== nothing
            return view
        end
        result = similar(A, m)
        for i in 1:m
            result[i] = A[i, 1]
        end
        return result
    else
        error("dropdims: dimension must be 1 or 2 for matrices")
    end
end

# =============================================================================
# @view macro - Transform A[i:j] to view(A, i:j)
# =============================================================================
# This macro is defined in macros.jl

# =============================================================================
# @views macro - Transform all indexing in a block to views
# =============================================================================
# This macro is defined in macros.jl
