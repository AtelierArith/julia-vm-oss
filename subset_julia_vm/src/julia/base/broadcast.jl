# =============================================================================
# broadcast.jl - Broadcasting infrastructure (Pure Julia)
# =============================================================================
# Based on Julia's base/broadcast.jl
#
# This module provides the Pure Julia broadcast types and functions:
# - BroadcastStyle type hierarchy
# - Broadcasted lazy wrapper
# - Extruded indexing struct
# - Materialization (copy, copyto!, materialize, materialize!)
#
# Phases implemented here:
#   Phase 1-2: Core types (BroadcastStyle, Broadcasted, shape computation)
#              Workaround: simplified non-parametric versions (Issue #2531, #2534, #2535, #2536)
#   Phase 3: Indexing infrastructure (Extruded, newindex, newindexer, _broadcast_getindex)
#            Issue #2537, #2538
#   Phase 4: Materialization (instantiate, materialize, copy, copyto!, similar, combine_eltypes,
#            preprocess, broadcast_unalias)
#            Issue #2539, #2540, #2541, #2542, #2543

# =============================================================================
# Phase 1-2 Workaround: BroadcastStyle type hierarchy
# =============================================================================
# Workaround: Simplified non-parametric BroadcastStyle (Issue #2531)
# In official Julia, BroadcastStyle is a complex parametric abstract type hierarchy.
# Here we use concrete structs with a `dims` field for the common cases.

abstract type BroadcastStyle end

struct DefaultArrayStyle <: BroadcastStyle
    dims::Int64
end

# Style{Tuple} equivalent for tuple broadcasting
struct TupleBroadcastStyle <: BroadcastStyle end

# Unknown style (for error cases)
struct BroadcastStyleUnknown <: BroadcastStyle end

# =============================================================================
# Phase 1-2 Workaround: Broadcasted struct
# =============================================================================
# Workaround: Non-parametric Broadcasted (Issue #2534)
# In official Julia, Broadcasted has 4 type parameters: Style, Axes, F, Args.
# Here we use a simple struct with untyped fields.

struct Broadcasted
    style     # BroadcastStyle or nothing
    f         # Function to apply
    # Workaround: field named 'bc_args' instead of 'args' to avoid compiler collision
    # with Expr.args field access (Issue #2534)
    bc_args   # Tuple of arguments
    axes_val  # Computed axes (tuple of ranges) or nothing
end

# Convenience constructor without axes (lazy, axes computed later)
Broadcasted(style, f, bc_args) = Broadcasted(style, f, bc_args, nothing)

# Convenience constructor without style
Broadcasted(f, bc_args) = Broadcasted(nothing, f, bc_args, nothing)

# axes for Broadcasted
function axes(bc::Broadcasted)
    if bc.axes_val !== nothing
        return bc.axes_val
    end
    # Compute axes from args if not yet instantiated
    return _broadcast_combine_axes(bc.bc_args)
end

# ndims for Broadcasted
function ndims(bc::Broadcasted)
    ax = axes(bc)
    return length(ax)
end

# length for Broadcasted
function length(bc::Broadcasted)
    ax = axes(bc)
    n = length(ax)
    result = 1
    for i in 1:n
        d = length(ax[i])
        result = result * d
    end
    return result
end

# eachindex for Broadcasted - returns a linear range
function eachindex(bc::Broadcasted)
    return 1:length(bc)
end

# getindex for Broadcasted with integer index
function getindex(bc::Broadcasted, I::Int64)
    return _broadcast_getindex(bc, I)
end

# getindex for Broadcasted with CartesianIndex
function getindex(bc::Broadcasted, I::CartesianIndex)
    return _broadcast_getindex(bc, I)
end

# =============================================================================
# Helper: check if a value is a broadcastable range (Issue #2686)
# =============================================================================
# In official Julia, LinRange <: AbstractRange. In SubsetJuliaVM, LinRange and
# StepRangeLen are structs (not native ranges), so isa(x, AbstractRange) is false.
# This helper checks for all range types.
function _is_broadcastable_range(x)
    if isa(x, AbstractRange)
        return true
    end
    t = typeof(x)
    # typeof returns e.g. LinRange{Float64}, StepRangeLen{Float64,...}
    # Check if the type name starts with "LinRange" or "StepRangeLen"
    s = string(t)
    if length(s) >= 8 && s[1:8] == "LinRange"
        return true
    end
    if length(s) >= 12 && s[1:12] == "StepRangeLen"
        return true
    end
    return false
end

# =============================================================================
# Phase 1-2 Workaround: Shape computation helpers
# =============================================================================
# Workaround: Simplified broadcast_shape / combine_axes (Issue #2535, #2536)

# Compute broadcast shape from two shapes
function _broadcast_shape(shape_a, shape_b)
    na = length(shape_a)
    nb = length(shape_b)
    ndim = max(na, nb)
    result = Vector{Int64}(undef, ndim)
    for i in 1:ndim
        da = i <= na ? shape_a[i] : 1
        db = i <= nb ? shape_b[i] : 1
        if da == db
            result[i] = da
        elseif da == 1
            result[i] = db
        elseif db == 1
            result[i] = da
        else
            throw(DimensionMismatch("arrays could not be broadcast to a common size; got a]dimension with lengths $da and $db"))
        end
    end
    # Convert to tuple
    n = length(result)
    if n == 0
        return ()
    elseif n == 1
        return (result[1],)
    elseif n == 2
        return (result[1], result[2])
    elseif n == 3
        return (result[1], result[2], result[3])
    elseif n == 4
        return (result[1], result[2], result[3], result[4])
    else
        return (result[1],)
    end
end

# Get the shape of a broadcastable argument
function _broadcastable_shape(x)
    if isa(x, Array)
        return size(x)
    elseif isa(x, SubArray)
        # A view participates in broadcast as the array it aliases (Issue #5137).
        return size(x)
    elseif isa(x, Tuple)
        return (length(x),)
    elseif isa(x, Broadcasted)
        ax = axes(x)
        n = length(ax)
        if n == 0
            # 0-dimensional nested broadcast: every operand of `x` is itself a
            # scalar (e.g. a fused `abs.(v .+ w)` whose inner `v .+ w` has only
            # static-array / scalar operands, which the generic shape system sees
            # as 0-dimensional). Return the scalar shape instead of indexing the
            # empty `ax` (`ax[1]` was an out-of-bounds crash — Issue #8176). The
            # outer `copy(::Broadcasted)` static hook then claims the broadcast.
            return ()
        elseif n == 1
            d1 = length(ax[1])
            return (d1,)
        elseif n == 2
            d1 = length(ax[1])
            d2 = length(ax[2])
            return (d1, d2)
        elseif n == 3
            d1 = length(ax[1])
            d2 = length(ax[2])
            d3 = length(ax[3])
            return (d1, d2, d3)
        else
            d1 = length(ax[1])
            return (d1,)
        end
    elseif _is_broadcastable_range(x)
        # UnitRange/StepRange/LinRange/StepRangeLen: 1D broadcastable collection (Issue #2686)
        return (length(x),)
    elseif isa(x, Ref)
        # Ref wraps a scalar for broadcast: 0-dimensional (Issue #2687)
        return ()
    else
        # Scalar
        return ()
    end
end

function _broadcastable_shape(x::SubArray{T,N,P,I,L}) where {T,N,P,I,L}
    return size(x)
end

# combine_axes: compute combined axes from broadcast arguments
function _broadcast_combine_axes(args)
    n = length(args)
    if n == 0
        return ()
    end
    shape = _broadcastable_shape(args[1])
    for i in 2:n
        next_shape = _broadcastable_shape(args[i])
        shape = _broadcast_shape(shape, next_shape)
    end
    # Convert shape to axes (tuple of ranges)
    nd = length(shape)
    if nd == 0
        return ()
    elseif nd == 1
        return (1:shape[1],)
    elseif nd == 2
        return (1:shape[1], 1:shape[2])
    elseif nd == 3
        return (1:shape[1], 1:shape[2], 1:shape[3])
    elseif nd == 4
        return (1:shape[1], 1:shape[2], 1:shape[3], 1:shape[4])
    else
        return (1:shape[1],)
    end
end

# check_broadcast_axes: verify that argument axes are compatible
function _check_broadcast_axes(target_axes, args)
    n = length(args)
    for i in 1:n
        arg_shape = _broadcastable_shape(args[i])
        target_ndim = length(target_axes)
        for d in 1:length(arg_shape)
            if d <= target_ndim
                target_len = length(target_axes[d])
                arg_len = arg_shape[d]
                if arg_len != 1 && arg_len != target_len
                    throw(DimensionMismatch("array could not be broadcast to match destination"))
                end
            end
        end
    end
    return nothing
end

# =============================================================================
# Phase 3-1: Extruded struct and newindex / newindexer (Issue #2537)
# =============================================================================
# Based on Julia's base/broadcast.jl L658-666
#
# Extruded wraps an array with information about which dimensions are "kept"
# (passed through normally) and which are "extruded" (use a default index).
# This is the key to efficient broadcasting: dimensions of size 1 are extruded.

struct Extruded
    x        # The wrapped array
    keeps    # Tuple of Bool: which dimensions to pass through
    defaults # Tuple of default indices for extruded dimensions
end

# axes for Extruded delegates to the wrapped array
function axes(b::Extruded)
    return axes(b.x)
end

# extrude: wrap an array with newindexer information
# Based on Julia's base/broadcast.jl L665-666
function extrude(x)
    if isa(x, Array)
        keeps, defaults = newindexer(x)
        return Extruded(x, keeps, defaults)
    elseif isa(x, SubArray)
        # A view broadcasts like the array it aliases (Issue #5137).
        keeps, defaults = newindexer(x)
        return Extruded(x, keeps, defaults)
    elseif _is_broadcastable_range(x)
        # Ranges behave like 1D arrays for broadcasting (Issue #2686)
        # For struct-based ranges (LinRange, StepRangeLen), compute keeps/defaults
        # directly since they don't support axes()/size() methods.
        rlen = length(x)
        keep = rlen != 1
        return Extruded(x, (keep,), (1,))
    else
        # Non-arrays (scalars, tuples, Ref, etc.) are returned as-is
        return x
    end
end

# newindexer: determine which dimensions to keep and default indices
# Based on Julia's base/broadcast.jl L604-611
function newindexer(A)
    return shapeindexer(axes(A))
end

# shapeindexer: convert axes to (keeps, defaults) tuples
function shapeindexer(ax)
    return _newindexer(ax)
end

# _newindexer: recursive helper for shapeindexer
# Uses runtime length checks instead of Tuple{} dispatch (SubsetJuliaVM pattern)
function _newindexer(indsA)
    n = length(indsA)
    if n == 0
        return (), ()
    end
    # Process the first axis
    ind1 = indsA[1]
    ind1_len = length(ind1)
    keep1 = ind1_len != 1
    default1 = first(ind1)
    # Process remaining axes
    if n == 1
        return (keep1,), (default1,)
    end
    rest = tail(indsA)
    rest_keeps, rest_defaults = _newindexer(rest)
    # Build keeps and defaults tuples
    nrest = length(rest_keeps)
    if nrest == 0
        return (keep1,), (default1,)
    elseif nrest == 1
        return (keep1, rest_keeps[1]), (default1, rest_defaults[1])
    elseif nrest == 2
        return (keep1, rest_keeps[1], rest_keeps[2]), (default1, rest_defaults[1], rest_defaults[2])
    elseif nrest == 3
        return (keep1, rest_keeps[1], rest_keeps[2], rest_keeps[3]), (default1, rest_defaults[1], rest_defaults[2], rest_defaults[3])
    else
        return (keep1, rest_keeps[1]), (default1, rest_defaults[1])
    end
end

# newindex: compute the actual index for a given linear/Cartesian index
# Based on Julia's base/broadcast.jl L583-600
#
# Combined into a single method with runtime isa check to avoid dispatch
# issues with CartesianIndex StructRef values (Issue #2689).
function newindex(I, keep, Idefault)
    if isa(I, CartesianIndex)
        # CartesianIndex: apply keeps/defaults to each dimension
        idx = I.I
        n = length(keep)
        if n == 0
            return CartesianIndex(())
        elseif n == 1
            i1 = ifelse(keep[1], idx[1], Idefault[1])
            return CartesianIndex((i1,))
        elseif n == 2
            i1 = ifelse(keep[1], length(idx) >= 1 ? idx[1] : Idefault[1], Idefault[1])
            i2 = ifelse(keep[2], length(idx) >= 2 ? idx[2] : Idefault[2], Idefault[2])
            return CartesianIndex((i1, i2))
        elseif n == 3
            i1 = ifelse(keep[1], length(idx) >= 1 ? idx[1] : Idefault[1], Idefault[1])
            i2 = ifelse(keep[2], length(idx) >= 2 ? idx[2] : Idefault[2], Idefault[2])
            i3 = ifelse(keep[3], length(idx) >= 3 ? idx[3] : Idefault[3], Idefault[3])
            return CartesianIndex((i1, i2, i3))
        else
            # Fallback for higher dimensions
            i1 = ifelse(keep[1], idx[1], Idefault[1])
            return CartesianIndex((i1,))
        end
    else
        # Integer index: if keep[d] is true, pass through; otherwise use default
        n = length(keep)
        if n == 0
            return I
        end
        if n == 1
            return ifelse(keep[1], I, Idefault[1])
        end
        # For multi-dimensional keeps with scalar index,
        # use the first keep/default pair
        return ifelse(keep[1], I, Idefault[1])
    end
end

# =============================================================================
# Phase 3-2: _broadcast_getindex (Issue #2538)
# =============================================================================
# Based on Julia's base/broadcast.jl L645-696
#
# _broadcast_getindex is the core element access function for broadcasting.
# It recursively evaluates the broadcast expression tree at a given index.

# _broadcast_getindex for real scalars (Real) - always return the scalar
function _broadcast_getindex(x::Real, I)
    return x
end

# _broadcast_getindex for Complex scalars (Issue #2691)
# Separate method needed because Complex is a struct (not a primitive numeric),
# so the compiler must use ReturnAny instead of ReturnF64.
function _broadcast_getindex(x::Complex, I)
    return x
end

# _broadcast_getindex for Bool (treated as scalar)
function _broadcast_getindex(x::Bool, I)
    return x
end

# _broadcast_getindex for Tuple - index into the tuple
function _broadcast_getindex(x::Tuple, I)
    if isa(I, CartesianIndex)
        return x[I.I[1]]
    else
        return x[I]
    end
end

# _broadcast_getindex for AbstractRange (UnitRange/StepRange) - index into the range (Issue #2686)
function _broadcast_getindex(r::AbstractRange, I)
    if isa(I, CartesianIndex)
        idx = I.I
        # Range is 1D: use first dimension, broadcast if length==1
        rlen = length(r)
        actual_idx = rlen == 1 ? 1 : idx[1]
        return r[actual_idx]
    else
        rlen = length(r)
        if rlen == 1
            return r[1]
        else
            return r[I]
        end
    end
end

# Note: Ref unwrapping is handled in _unwrap_ref (called from _getindex) rather than
# as a _broadcast_getindex method, because Ref's runtime type is the inner value's type
# (e.g., Ref(10) has runtime type Int64), so dispatch would not match ::Ref (Issue #2687)

# _broadcast_getindex for Array - compute the broadcast index
function _broadcast_getindex(A::Array, I)
    if isa(I, CartesianIndex)
        # Multi-dimensional: compute linear index with broadcasting
        idx = I.I
        s = size(A)
        ndim_a = length(s)
        ndim_i = length(idx)
        # Compute linear index with dimension broadcasting
        linear_idx = 1
        stride = 1
        for d in 1:max(ndim_a, ndim_i)
            dim_size = d <= ndim_a ? s[d] : 1
            i_d = d <= ndim_i ? idx[d] : 1
            # Broadcasting: if dim_size == 1, use index 1
            actual_idx = dim_size == 1 ? 1 : i_d
            linear_idx = linear_idx + (actual_idx - 1) * stride
            stride = stride * dim_size
        end
        return A[linear_idx]
    else
        # Linear index: simple case for 1D
        s = size(A)
        if length(s) == 1
            # 1D array: direct index, but broadcast if length==1
            if s[1] == 1
                return A[1]
            else
                return A[I]
            end
        else
            # Multi-dim array with linear index
            return A[I]
        end
    end
end

# A SubArray view indexes like a plain Array (it supports `size` plus linear and
# Cartesian access). This handles the non-preprocessed path — e.g. the generic
# `copyto!(dest, src)` used for a BitVector destination from a comparison
# broadcast like `view(v, a:b) .> 2`, which calls `getindex(bc, i)` on the raw
# (un-Extruded) view rather than going through the Extruded loop (Issue #5137).
function _broadcast_getindex(A::SubArray{T,N,P,I,L}, idx) where {T,N,P,I,L}
    if isa(idx, CartesianIndex)
        ii = idx.I
        s = size(A)
        ndim_a = length(s)
        ndim_i = length(ii)
        linear_idx = 1
        stride = 1
        for d in 1:max(ndim_a, ndim_i)
            dim_size = d <= ndim_a ? s[d] : 1
            i_d = d <= ndim_i ? ii[d] : 1
            actual_idx = dim_size == 1 ? 1 : i_d
            linear_idx = linear_idx + (actual_idx - 1) * stride
            stride = stride * dim_size
        end
        return A[linear_idx]
    else
        s = size(A)
        if length(s) == 1 && s[1] == 1
            return A[1]
        else
            return A[idx]
        end
    end
end

# _broadcast_getindex for Extruded - use newindex to compute the actual index
function _broadcast_getindex(b::Extruded, I)
    actual_idx = newindex(I, b.keeps, b.defaults)
    if isa(actual_idx, CartesianIndex)
        # Convert CartesianIndex to linear index for array access
        idx = actual_idx.I
        s = size(b.x)
        ndim = length(s)
        linear = 1
        stride = 1
        for d in 1:min(ndim, length(idx))
            linear = linear + (idx[d] - 1) * stride
            stride = stride * s[d]
        end
        return b.x[linear]
    else
        return b.x[actual_idx]
    end
end

# _broadcast_getindex for Broadcasted - recursively evaluate the expression tree
function _broadcast_getindex(bc::Broadcasted, I)
    # Get each argument at index I
    bc_a = bc.bc_args
    args = _getindex(bc_a, I)
    # Apply the function to the collected arguments
    return _broadcast_apply(bc.f, args)
end

# _unwrap_ref: unwrap Ref values after _broadcast_getindex (Issue #2687)
# Ref(x) is treated as 0-dimensional scalar by the broadcast shape system,
# so _broadcast_getindex returns the Ref unchanged. We unwrap here before
# passing to the operator function.
function _unwrap_ref(x)
    if isa(x, Ref)
        return getindex(x)
    end
    return x
end

# _getindex_one: get broadcast-indexed value from a single argument
# Handles special types that can't dispatch through _broadcast_getindex:
# - Ref: runtime_type is inner value's type, causing misrouted dispatch
# - LinRange/StepRangeLen: struct types that don't match ::AbstractRange annotation
# - Complex/Rational/other struct scalars: StructRef has runtime_type Any, misroutes dispatch
# Strategy: only dispatch to _broadcast_getindex for types we know work correctly
# (Number, Bool, Tuple, AbstractRange, Array, Extruded, Broadcasted).
# Everything else (struct scalars, Ref) is handled directly here.
function _getindex_one(arg, I)
    if isa(arg, Ref)
        return getindex(arg)
    end
    # Types that dispatch correctly through _broadcast_getindex
    if isa(arg, Number) || isa(arg, Bool)
        return arg  # Scalar: return as-is
    end
    if isa(arg, Array)
        return _broadcast_getindex(arg, I)
    end
    if isa(arg, SubArray)
        # A view indexes like an Array; route to its _broadcast_getindex so the
        # non-Extruded path (e.g. a BitVector comparison destination) reads the
        # aliased elements instead of treating the whole view as a scalar (#5137).
        return _broadcast_getindex(arg, I)
    end
    if isa(arg, Tuple)
        return _broadcast_getindex(arg, I)
    end
    if isa(arg, AbstractRange)
        return _broadcast_getindex(arg, I)
    end
    if isa(arg, Extruded)
        return _broadcast_getindex(arg, I)
    end
    if isa(arg, Broadcasted)
        return _broadcast_getindex(arg, I)
    end
    if _is_broadcastable_range(arg)
        # Struct-based ranges (LinRange, StepRangeLen)
        rlen = length(arg)
        if rlen == 1
            return arg[1]
        else
            return arg[I]
        end
    end
    # Default: treat as scalar (Complex, Rational, other struct types)
    return arg
end

# _getindex: collect broadcast-indexed values from a tuple of arguments
function _getindex(args, I)
    n = length(args)
    if n == 0
        return ()
    elseif n == 1
        a1 = _getindex_one(args[1], I)
        return (a1,)
    elseif n == 2
        a1 = _getindex_one(args[1], I)
        a2 = _getindex_one(args[2], I)
        return (a1, a2)
    elseif n == 3
        a1 = _getindex_one(args[1], I)
        a2 = _getindex_one(args[2], I)
        a3 = _getindex_one(args[3], I)
        return (a1, a2, a3)
    elseif n == 4
        a1 = _getindex_one(args[1], I)
        a2 = _getindex_one(args[2], I)
        a3 = _getindex_one(args[3], I)
        a4 = _getindex_one(args[4], I)
        return (a1, a2, a3, a4)
    else
        # Fallback: handle first 4 args
        a1 = _getindex_one(args[1], I)
        a2 = _getindex_one(args[2], I)
        a3 = _getindex_one(args[3], I)
        a4 = _getindex_one(args[4], I)
        return (a1, a2, a3, a4)
    end
end

# =============================================================================
# Multi-dimensional broadcast getindex helpers (Issue #2686)
# =============================================================================
# These avoid using CartesianIndex objects (which cause StructRef dispatch issues)
# by passing individual dimension indices as plain integers.

# --- 2D helpers ---

# _broadcast_getindex_2d for scalars
function _broadcast_getindex_2d(x::Number, i, j)
    return x
end

function _broadcast_getindex_2d(x::Bool, i, j)
    return x
end

# _broadcast_getindex_2d for Tuple (1D: use first dimension index)
function _broadcast_getindex_2d(x::Tuple, i, j)
    tlen = length(x)
    if tlen == 1
        return x[1]
    else
        return x[i]
    end
end

# _broadcast_getindex_2d for AbstractRange (1D: broadcasting rules)
function _broadcast_getindex_2d(r::AbstractRange, i, j)
    rlen = length(r)
    if rlen == 1
        return r[1]
    else
        return r[i]
    end
end

# _broadcast_getindex_2d for Array: compute linear index with 2D broadcasting
function _broadcast_getindex_2d(A::Array, i, j)
    s = size(A)
    ndim = length(s)
    if ndim == 1
        # 1D array broadcast into 2D: use dim1, broadcast dim2
        actual_i = s[1] == 1 ? 1 : i
        return A[actual_i]
    else
        # 2D array: compute linear index with broadcasting
        actual_i = s[1] == 1 ? 1 : i
        actual_j = ndim >= 2 && s[2] == 1 ? 1 : j
        linear = actual_i + (actual_j - 1) * s[1]
        return A[linear]
    end
end

# _broadcast_getindex_2d for Extruded: apply keeps/defaults per dimension
function _broadcast_getindex_2d(b::Extruded, i, j)
    keeps = b.keeps
    defaults = b.defaults
    nk = length(keeps)
    if nk == 0
        return b.x[1]
    elseif nk == 1
        actual_i = keeps[1] ? i : defaults[1]
        return b.x[actual_i]
    else
        actual_i = keeps[1] ? i : defaults[1]
        actual_j = keeps[2] ? j : defaults[2]
        s = size(b.x)
        linear = actual_i + (actual_j - 1) * s[1]
        return b.x[linear]
    end
end

# _broadcast_getindex_2d for Broadcasted: recursively evaluate with 2D indices
function _broadcast_getindex_2d(bc::Broadcasted, i, j)
    bc_a = bc.bc_args
    args = _getindex_2d(bc_a, i, j)
    return _broadcast_apply(bc.f, args)
end

# _getindex_2d: collect 2D broadcast-indexed values from a tuple of arguments
function _getindex_2d(args, i, j)
    n = length(args)
    if n == 0
        return ()
    elseif n == 1
        a1 = _getindex_one_2d(args[1], i, j)
        return (a1,)
    elseif n == 2
        a1 = _getindex_one_2d(args[1], i, j)
        a2 = _getindex_one_2d(args[2], i, j)
        return (a1, a2)
    elseif n == 3
        a1 = _getindex_one_2d(args[1], i, j)
        a2 = _getindex_one_2d(args[2], i, j)
        a3 = _getindex_one_2d(args[3], i, j)
        return (a1, a2, a3)
    elseif n == 4
        a1 = _getindex_one_2d(args[1], i, j)
        a2 = _getindex_one_2d(args[2], i, j)
        a3 = _getindex_one_2d(args[3], i, j)
        a4 = _getindex_one_2d(args[4], i, j)
        return (a1, a2, a3, a4)
    else
        a1 = _getindex_one_2d(args[1], i, j)
        a2 = _getindex_one_2d(args[2], i, j)
        a3 = _getindex_one_2d(args[3], i, j)
        a4 = _getindex_one_2d(args[4], i, j)
        return (a1, a2, a3, a4)
    end
end

# _getindex_one_2d: get broadcast-indexed value from a single argument (2D)
function _getindex_one_2d(arg, i, j)
    if isa(arg, Ref)
        return getindex(arg)
    end
    if isa(arg, Number) || isa(arg, Bool)
        return arg
    end
    if isa(arg, Array)
        return _broadcast_getindex_2d(arg, i, j)
    end
    if isa(arg, Tuple)
        tlen = length(arg)
        return tlen == 1 ? arg[1] : arg[i]
    end
    if isa(arg, AbstractRange)
        rlen = length(arg)
        return rlen == 1 ? arg[1] : arg[i]
    end
    if isa(arg, Extruded)
        return _broadcast_getindex_2d(arg, i, j)
    end
    if isa(arg, Broadcasted)
        return _broadcast_getindex_2d(arg, i, j)
    end
    if _is_broadcastable_range(arg)
        rlen = length(arg)
        return rlen == 1 ? arg[1] : arg[i]
    end
    # Default: scalar (Complex, Rational, other struct types)
    return arg
end

# --- 3D helpers ---

function _broadcast_getindex_3d(x::Number, i, j, k)
    return x
end

function _broadcast_getindex_3d(x::Bool, i, j, k)
    return x
end

function _broadcast_getindex_3d(A::Array, i, j, k)
    s = size(A)
    ndim = length(s)
    actual_i = s[1] == 1 ? 1 : i
    actual_j = ndim >= 2 ? (s[2] == 1 ? 1 : j) : 1
    actual_k = ndim >= 3 ? (s[3] == 1 ? 1 : k) : 1
    linear = actual_i + (actual_j - 1) * s[1] + (actual_k - 1) * s[1] * (ndim >= 2 ? s[2] : 1)
    return A[linear]
end

function _broadcast_getindex_3d(b::Extruded, i, j, k)
    keeps = b.keeps
    defaults = b.defaults
    nk = length(keeps)
    if nk == 0
        return b.x[1]
    elseif nk == 1
        actual_i = keeps[1] ? i : defaults[1]
        return b.x[actual_i]
    elseif nk == 2
        actual_i = keeps[1] ? i : defaults[1]
        actual_j = keeps[2] ? j : defaults[2]
        s = size(b.x)
        linear = actual_i + (actual_j - 1) * s[1]
        return b.x[linear]
    else
        actual_i = keeps[1] ? i : defaults[1]
        actual_j = keeps[2] ? j : defaults[2]
        actual_k = keeps[3] ? k : defaults[3]
        s = size(b.x)
        linear = actual_i + (actual_j - 1) * s[1] + (actual_k - 1) * s[1] * s[2]
        return b.x[linear]
    end
end

function _broadcast_getindex_3d(bc::Broadcasted, i, j, k)
    bc_a = bc.bc_args
    args = _getindex_3d(bc_a, i, j, k)
    return _broadcast_apply(bc.f, args)
end

function _getindex_3d(args, i, j, k)
    n = length(args)
    if n == 0
        return ()
    elseif n == 1
        a1 = _getindex_one_3d(args[1], i, j, k)
        return (a1,)
    elseif n == 2
        a1 = _getindex_one_3d(args[1], i, j, k)
        a2 = _getindex_one_3d(args[2], i, j, k)
        return (a1, a2)
    elseif n == 3
        a1 = _getindex_one_3d(args[1], i, j, k)
        a2 = _getindex_one_3d(args[2], i, j, k)
        a3 = _getindex_one_3d(args[3], i, j, k)
        return (a1, a2, a3)
    else
        a1 = _getindex_one_3d(args[1], i, j, k)
        a2 = _getindex_one_3d(args[2], i, j, k)
        a3 = _getindex_one_3d(args[3], i, j, k)
        a4 = _getindex_one_3d(args[4], i, j, k)
        return (a1, a2, a3, a4)
    end
end

function _getindex_one_3d(arg, i, j, k)
    if isa(arg, Ref)
        return getindex(arg)
    end
    if isa(arg, Number) || isa(arg, Bool)
        return arg
    end
    if isa(arg, Array)
        return _broadcast_getindex_3d(arg, i, j, k)
    end
    if isa(arg, Tuple)
        tlen = length(arg)
        return tlen == 1 ? arg[1] : arg[i]
    end
    if isa(arg, AbstractRange)
        rlen = length(arg)
        return rlen == 1 ? arg[1] : arg[i]
    end
    if isa(arg, Extruded)
        return _broadcast_getindex_3d(arg, i, j, k)
    end
    if isa(arg, Broadcasted)
        return _broadcast_getindex_3d(arg, i, j, k)
    end
    if _is_broadcastable_range(arg)
        rlen = length(arg)
        return rlen == 1 ? arg[1] : arg[i]
    end
    return arg
end

# _broadcast_apply: apply function to collected arguments tuple
function _broadcast_apply(f, args)
    n = length(args)
    if n == 0
        return f()
    elseif n == 1
        return f(args[1])
    elseif n == 2
        return f(args[1], args[2])
    elseif n == 3
        return f(args[1], args[2], args[3])
    elseif n == 4
        return f(args[1], args[2], args[3], args[4])
    else
        return f(args[1], args[2], args[3], args[4])
    end
end

# =============================================================================
# Phase 4-1: instantiate (Issue #2539)
# =============================================================================
# Based on Julia's base/broadcast.jl L308-323
#
# instantiate finalizes a Broadcasted object by computing its axes.
# If axes are already set, it validates them against the arguments.

function instantiate(bc::Broadcasted)
    if bc.axes_val === nothing
        # Compute axes from args
        computed_axes = _broadcast_combine_axes(bc.bc_args)
        return Broadcasted(bc.style, bc.f, bc.bc_args, computed_axes)
    else
        # Axes already set, validate
        _check_broadcast_axes(bc.axes_val, bc.bc_args)
        return bc
    end
end

# instantiate for non-Broadcasted values: pass through
# INTENTIONAL_NOOP (Issue #4703): upstream `instantiate(x) = x`
# (julia/base/broadcast.jl:297) is the generic pass-through fallback for
# any non-Broadcasted value; the typed `instantiate(bc::Broadcasted)`
# above does the real work. A `return x` body is correct, not a stub.
function instantiate(x)
    return x
end

# =============================================================================
# Phase 4-4: similar(::Broadcasted) / combine_eltypes (Issue #2542)
# =============================================================================
# Based on Julia's base/broadcast.jl L227-236, L737-749
#
# combine_eltypes infers the result element type from the function and argument types.
# similar allocates the output array for a Broadcasted.

# combine_eltypes: infer the result element type
# Simplified version: uses runtime sampling to determine the type
function combine_eltypes(f, args)
    # Try to determine element types from args
    # For now, sample with a representative element to infer type
    n = length(args)
    if n == 0
        return Any
    end
    arithmetic_name = string(f)
    if arithmetic_name == "+" || arithmetic_name == "function +" ||
       arithmetic_name == "-" || arithmetic_name == "function -" ||
       arithmetic_name == "*" || arithmetic_name == "function *"
        arithmetic_type = _same_broadcast_arithmetic_eltype(args)
        if arithmetic_type !== nothing
            return arithmetic_type
        end
    elseif arithmetic_name == "/" || arithmetic_name == "function /"
        float_type = _same_broadcast_float_eltype(args)
        if float_type !== nothing
            return float_type
        end
    end
    # Get a representative element from each arg
    sample_args = _get_sample_elements(args)
    # Apply the function to sample values and check the result type
    result = _broadcast_apply(f, sample_args)
    if isa(result, Int8)
        return Int8
    elseif isa(result, Int16)
        return Int16
    elseif isa(result, Int32)
        return Int32
    elseif isa(result, Int64)
        return Int64
    elseif isa(result, UInt8)
        return UInt8
    elseif isa(result, UInt16)
        return UInt16
    elseif isa(result, UInt32)
        return UInt32
    elseif isa(result, UInt64)
        return UInt64
    elseif isa(result, Float32)
        return Float32
    elseif isa(result, Float64)
        return Float64
    elseif isa(result, Bool)
        return Bool
    elseif isa(result, String)
        return String
    elseif isa(result, Char)
        return Char
    elseif isa(result, Complex)
        # Complex results need proper complex-typed arrays (Issue #2688)
        return Complex{Float64}
    else
        return Any
    end
end

function _broadcast_arg_eltype(arg)
    if isa(arg, Array)
        return eltype(arg)
    elseif isa(arg, SubArray)
        # A view contributes its element type, not the SubArray wrapper type,
        # so a broadcast like `view(v, a:b) .+ 1` infers the arithmetic eltype
        # and avoids a scalar `+(::SubArray, ::Int)` apply (Issue #5137).
        return eltype(arg)
    elseif isa(arg, Ref)
        return typeof(getindex(arg))
    elseif isa(arg, Broadcasted)
        return combine_eltypes(arg.f, arg.bc_args)
    else
        return typeof(arg)
    end
end

function _same_broadcast_eltype(args)
    n = length(args)
    if n == 0
        return nothing
    end
    first_type = _broadcast_arg_eltype(args[1])
    for i in 2:n
        if _broadcast_arg_eltype(args[i]) != first_type
            return nothing
        end
    end
    return first_type
end

function _same_broadcast_arithmetic_eltype(args)
    T = _same_broadcast_eltype(args)
    if T == Int8 || T == Int16 || T == Int32 || T == UInt8 ||
       T == UInt16 || T == UInt32 || T == UInt64 || T == Float32 ||
       T == Float64
        return T
    end
    return nothing
end

function _same_broadcast_float_eltype(args)
    T = _same_broadcast_eltype(args)
    if T == Float32 || T == Float64
        return T
    end
    return nothing
end

# Helper: get a sample element from each argument
function _get_sample_elements(args)
    n = length(args)
    if n == 0
        return ()
    elseif n == 1
        a1 = _get_first_element(args[1])
        return (a1,)
    elseif n == 2
        a1 = _get_first_element(args[1])
        a2 = _get_first_element(args[2])
        return (a1, a2)
    elseif n == 3
        a1 = _get_first_element(args[1])
        a2 = _get_first_element(args[2])
        a3 = _get_first_element(args[3])
        return (a1, a2, a3)
    elseif n == 4
        a1 = _get_first_element(args[1])
        a2 = _get_first_element(args[2])
        a3 = _get_first_element(args[3])
        a4 = _get_first_element(args[4])
        return (a1, a2, a3, a4)
    else
        a1 = _get_first_element(args[1])
        a2 = _get_first_element(args[2])
        return (a1, a2)
    end
end

# Helper: get the first element of a broadcastable value
function _get_first_element(x)
    if isa(x, Array)
        if length(x) > 0
            return x[1]
        else
            return 0  # Fallback
        end
    elseif isa(x, SubArray)
        # A view's representative element drives broadcast eltype inference, so
        # `view(v, a:b) .+ 1` samples `v[1]` rather than the whole view (#5137).
        if length(x) > 0
            return x[1]
        else
            return 0
        end
    elseif isa(x, Tuple)
        if length(x) > 0
            return x[1]
        else
            return 0
        end
    elseif isa(x, Broadcasted)
        return _broadcast_getindex(x, 1)
    elseif isa(x, Extruded)
        return _get_first_element(x.x)
    elseif _is_broadcastable_range(x)
        # Range (UnitRange/LinRange/StepRangeLen): return first element (Issue #2686)
        return first(x)
    elseif isa(x, Ref)
        # Ref: unwrap the contained value (Issue #2687)
        # Use getindex(x) instead of x[] because x[] is not correctly lowered for Ref
        return getindex(x)
    else
        # Scalar
        return x
    end
end

# similar for Broadcasted: allocate output array
# Note: Vector{ElType}(undef, n) with a runtime ElType variable creates Vector{Any}
# because the compiler can't resolve runtime type variables at compile time.
# Instead, we use explicit compile-time type literals for each known type.
function _broadcasted_similar(bc::Broadcasted, ElType)
    ax = axes(bc)
    nd = length(ax)
    tname = string(ElType)
    if tname == "Bool"
        dims = ()
        for axis in ax
            dims = tuple(dims..., length(axis))
        end
        return _mark_bitarray(_array_undef_from_dims(Bool, dims))
    end
    if nd == 2
        d1 = length(ax[1])
        d2 = length(ax[2])
        if tname == "Float64"
            return Array{Float64}(undef, d1, d2)
        elseif tname == "Float32"
            return Array{Float32}(undef, d1, d2)
        elseif tname == "Int8"
            return Array{Int8}(undef, d1, d2)
        elseif tname == "Int16"
            return Array{Int16}(undef, d1, d2)
        elseif tname == "Int32"
            return Array{Int32}(undef, d1, d2)
        elseif tname == "Int64"
            return Array{Int64}(undef, d1, d2)
        elseif tname == "UInt8"
            return Array{UInt8}(undef, d1, d2)
        elseif tname == "UInt16"
            return Array{UInt16}(undef, d1, d2)
        elseif tname == "UInt32"
            return Array{UInt32}(undef, d1, d2)
        elseif tname == "UInt64"
            return Array{UInt64}(undef, d1, d2)
        elseif tname == "String"
            return Array{String}(undef, d1, d2)
        elseif tname == "Char"
            return Array{Char}(undef, d1, d2)
        elseif length(tname) >= 7 && tname[1:7] == "Complex"
            return Array{Complex{Float64}}(undef, d1, d2)
        else
            return Array{Any}(undef, d1, d2)
        end
    end

    # Calculate total element count
    if nd == 0
        n = 1
    elseif nd == 1
        d1 = length(ax[1])
        n = d1
    elseif nd == 2
        d1 = length(ax[1])
        d2 = length(ax[2])
        n = d1 * d2
    else
        n = 1
        for i in 1:nd
            d = length(ax[i])
            n = n * d
        end
    end
    # Create typed array using compile-time type literals
    if tname == "Float64"
        arr = Vector{Float64}(undef, n)
    elseif tname == "Float32"
        arr = Vector{Float32}(undef, n)
    elseif tname == "Int8"
        arr = Vector{Int8}(undef, n)
    elseif tname == "Int16"
        arr = Vector{Int16}(undef, n)
    elseif tname == "Int32"
        arr = Vector{Int32}(undef, n)
    elseif tname == "Int64"
        arr = Vector{Int64}(undef, n)
    elseif tname == "UInt8"
        arr = Vector{UInt8}(undef, n)
    elseif tname == "UInt16"
        arr = Vector{UInt16}(undef, n)
    elseif tname == "UInt32"
        arr = Vector{UInt32}(undef, n)
    elseif tname == "UInt64"
        arr = Vector{UInt64}(undef, n)
    elseif tname == "String"
        arr = Vector{String}(undef, n)
    elseif tname == "Char"
        arr = Vector{Char}(undef, n)
    elseif length(tname) >= 7 && tname[1:7] == "Complex"
        arr = Vector{Complex{Float64}}(undef, n)
    else
        arr = Vector{Any}(undef, n)
    end
    # Reshape for 2D
    if nd == 2
        d1 = length(ax[1])
        d2 = length(ax[2])
        return reshape(arr, d1, d2)
    end
    return arr
end

function similar(bc::Broadcasted, ElType::Type)
    return _broadcasted_similar(bc, ElType)
end

function similar(bc::Broadcasted, ElType)
    return _broadcasted_similar(bc, ElType)
end

# =============================================================================
# Phase 4-5: preprocess / broadcast_unalias (Issue #2543)
# =============================================================================
# Based on Julia's base/broadcast.jl L967-978
#
# preprocess prepares a Broadcasted for execution by:
# 1. Checking for aliasing between destination and source
# 2. Wrapping arrays in Extruded for efficient index mapping

# broadcast_unalias: check if dest and src are the same object
function _broadcast_array_copy(src::Array)
    n = length(src)
    T = eltype(src)
    tname = string(T)
    if tname == "Float64"
        result = Vector{Float64}(undef, n)
    elseif tname == "Float32"
        result = Vector{Float32}(undef, n)
    elseif tname == "Int8"
        result = Vector{Int8}(undef, n)
    elseif tname == "Int16"
        result = Vector{Int16}(undef, n)
    elseif tname == "Int32"
        result = Vector{Int32}(undef, n)
    elseif tname == "Int64"
        result = Vector{Int64}(undef, n)
    elseif tname == "UInt8"
        result = Vector{UInt8}(undef, n)
    elseif tname == "UInt16"
        result = Vector{UInt16}(undef, n)
    elseif tname == "UInt32"
        result = Vector{UInt32}(undef, n)
    elseif tname == "UInt64"
        result = Vector{UInt64}(undef, n)
    elseif tname == "Bool"
        result = Vector{Bool}(undef, n)
    elseif tname == "String"
        result = Vector{String}(undef, n)
    elseif tname == "Char"
        result = Vector{Char}(undef, n)
    elseif length(tname) >= 7 && tname[1:7] == "Complex"
        result = Vector{Complex{Float64}}(undef, n)
    else
        result = Vector{Any}(undef, n)
    end
    for i in 1:n
        value = src[i]
        result[i] = value
    end
    s = size(src)
    nd = length(s)
    if nd == 2
        return reshape(result, s[1], s[2])
    elseif nd == 3
        return reshape(result, s[1], s[2], s[3])
    end
    return result
end

function broadcast_unalias(dest, src)
    if dest === src
        # Same object: make a copy to avoid aliasing
        if isa(src, Array)
            return _broadcast_array_copy(src)
        end
        return copy(src)
    else
        return src
    end
end

# broadcast_unalias with nothing destination (no aliasing possible)
function broadcast_unalias(dest::Nothing, src)
    return src
end

# preprocess for Broadcasted: recursively preprocess all arguments
function preprocess(dest, bc::Broadcasted)
    new_args = _preprocess_args(dest, bc.bc_args)
    return Broadcasted(bc.style, bc.f, new_args, bc.axes_val)
end

# preprocess for non-Broadcasted values: extrude arrays
function preprocess(dest, x)
    return extrude(broadcast_unalias(dest, x))
end

# _preprocess_args: preprocess each argument in a tuple
function _preprocess_args(dest, args)
    n = length(args)
    if n == 0
        return ()
    elseif n == 1
        a1 = preprocess(dest, args[1])
        return (a1,)
    elseif n == 2
        a1 = preprocess(dest, args[1])
        a2 = preprocess(dest, args[2])
        return (a1, a2)
    elseif n == 3
        a1 = preprocess(dest, args[1])
        a2 = preprocess(dest, args[2])
        a3 = preprocess(dest, args[3])
        return (a1, a2, a3)
    elseif n == 4
        a1 = preprocess(dest, args[1])
        a2 = preprocess(dest, args[2])
        a3 = preprocess(dest, args[3])
        a4 = preprocess(dest, args[4])
        return (a1, a2, a3, a4)
    else
        a1 = preprocess(dest, args[1])
        a2 = preprocess(dest, args[2])
        return (a1, a2)
    end
end

# =============================================================================
# Phase 4-3: copy / copyto! for Broadcasted (Issue #2541)
# =============================================================================
# Based on Julia's base/broadcast.jl L908-997
#
# copy creates a new array from a Broadcasted.
# copyto! fills an existing array from a Broadcasted.

# copy for Broadcasted: allocate result and fill it
# Extension hook (Issue #7460): a StaticArrays-style package installs a
# `(f, args) -> value-or-nothing` callback here so that `copy(::Broadcasted)` can
# return a value of the package's own fixed-shape type (e.g. SVector/SMatrix)
# before the generic pipeline runs. It is stored in a `Ref` rather than as an
# overridable method on purpose: a base-internal *named* call devirtualises to
# the default and never sees a package's override, whereas reading the callback
# out of this Ref yields a true runtime function value that dispatches into the
# package. `nothing` (the default) means "no package loaded" — ordinary
# broadcasts are unaffected.
const _STATIC_BROADCAST_HOOK = Ref{Any}(nothing)

# Cross-module accessors (a package can call `Base._set_static_broadcast_hook!`
# but cannot read the raw const global). The getter is a normal same-module call
# inside `copy`; it always returns the Ref's *current* content, so devirtualising
# it is harmless — the package-installed value flows through unchanged.
_set_static_broadcast_hook!(f) = (_STATIC_BROADCAST_HOOK[] = f; nothing)
_get_static_broadcast_hook() = _STATIC_BROADCAST_HOOK[]

# Cross-module `Broadcasted` introspection for the static-array hook
# (StaticArrays/src/broadcast.jl, Issue #8161). sjulia fuses a `.`-call chain
# (`abs.(v .- w)`) into a *tree* of nested `Broadcasted` nodes, so a package's
# hook must be able to detect and walk that tree without naming the Base type or
# reaching into its fields cross-module. `_materialize_broadcasted` re-runs the
# generic pipeline on a freshly built `Broadcasted` (a same-module call so
# `copy(::Broadcasted)` resolves cleanly) — used for the dynamic-result path of a
# mixed static/dynamic broadcast.
_is_broadcasted(x) = isa(x, Broadcasted)
_broadcasted_f(bc) = bc.f
_broadcasted_args(bc) = bc.bc_args
_make_broadcasted(f, args) = Broadcasted(f, args)
_materialize_broadcasted(f, args) = copy(Broadcasted(f, args))

function copy(bc::Broadcasted)
    # Static-array fast path (Issue #7460): a loaded StaticArrays-style package
    # can claim the broadcast and return a static result. Runs before the generic
    # pipeline so a static operand is not mis-treated as a 0-dimensional scalar
    # (which would collapse `v .+ 10` to the invalid `+(v, 10)`).
    hook = _get_static_broadcast_hook()
    if hook !== nothing
        static_result = hook(bc.f, bc.bc_args)
        if static_result !== nothing
            return static_result
        end
    end
    ibc = instantiate(bc)
    # 0-dimensional broadcast (all scalar operands): return scalar result (Issue #4)
    ax = axes(ibc)
    if length(ax) == 0
        args = _getindex(ibc.bc_args, 1)
        return _broadcast_apply(ibc.f, args)
    end
    ElType = combine_eltypes(ibc.f, ibc.bc_args)
    dest = similar(ibc, ElType)
    return copyto!(dest, ibc)
end

# Fast path for same-shape 1D binary array broadcasts.
# Conditions:
# - destination and both arguments are 1D Arrays of equal length
# - no aliasing between destination and source arrays
#
# Optimization strategy:
# - Float64 +,-,*,/ use direct arithmetic loop
# - otherwise, use generic element-wise function application while skipping
#   generic broadcast index/preprocess machinery
#
# Returns true when fast path was applied, false otherwise.
function _copyto_fastpath_same_shape_binary!(dest::Array, bc::Broadcasted)
    if length(size(dest)) != 1
        return false
    end

    args = bc.bc_args
    if length(args) != 2
        return false
    end

    a = args[1]
    b = args[2]
    if !(isa(a, Array) && isa(b, Array))
        return false
    end
    if length(size(a)) != 1 || length(size(b)) != 1
        return false
    end
    # Preserve semantics for aliasing cases by falling back to generic preprocess path.
    if dest === a || dest === b
        return false
    end

    n = length(dest)
    if length(a) != n || length(b) != n
        return false
    end

    # Fastest kernel: Float64 same-type arithmetic
    if eltype(dest) == Float64 && eltype(a) == Float64 && eltype(b) == Float64
        f_name = string(bc.f)
        if f_name == "+" || f_name == "function +"
            for i in 1:n
                dest[i] = a[i] + b[i]
            end
            return true
        elseif f_name == "-" || f_name == "function -"
            for i in 1:n
                dest[i] = a[i] - b[i]
            end
            return true
        elseif f_name == "*" || f_name == "function *"
            for i in 1:n
                dest[i] = a[i] * b[i]
            end
            return true
        elseif f_name == "/" || f_name == "function /"
            for i in 1:n
                dest[i] = a[i] / b[i]
            end
            return true
        end
    end

    # Generic same-shape binary path (covers Int64 and other typed arrays).
    # This still performs dynamic function application, but skips expensive
    # broadcast index/extrusion machinery.
    f = bc.f
    for i in 1:n
        value = _broadcast_apply(f, (a[i], b[i]))
        dest[i] = value
    end
    return true
end

function _fastpath_unwrap_scalar_arg(x)
    if isa(x, Ref)
        return (true, getindex(x))
    end
    if isa(x, Array) || isa(x, Tuple) || isa(x, Broadcasted) || _is_broadcastable_range(x)
        return (false, nothing)
    end
    return (true, x)
end

# Fast path for array/range and scalar (or Ref scalar) binary broadcasts.
# This avoids preprocess/extrusion/cartesian overhead, including for 2D arrays.
function _copyto_fastpath_array_scalar!(dest::Array, bc::Broadcasted)
    args = bc.bc_args
    if length(args) != 2
        return false
    end

    left = args[1]
    right = args[2]

    # A SubArray view is array-like but not a native `Array`, so the scalar
    # fast path would mis-classify it as the scalar operand; defer to the
    # generic Extruded loop, which indexes the view correctly (Issue #5137).
    if isa(left, SubArray) || isa(right, SubArray)
        return false
    end

    arr = nothing
    scalar = nothing
    scalar_left = false

    if isa(left, Array) || _is_broadcastable_range(left)
        ok, s = _fastpath_unwrap_scalar_arg(right)
        if !ok
            return false
        end
        arr = left
        scalar = s
        scalar_left = false
    elseif isa(right, Array) || _is_broadcastable_range(right)
        ok, s = _fastpath_unwrap_scalar_arg(left)
        if !ok
            return false
        end
        arr = right
        scalar = s
        scalar_left = true
    else
        return false
    end

    n = length(dest)
    if length(arr) != n
        return false
    end

    if isa(arr, Array)
        s_dest = size(dest)
        s_arr = size(arr)
        if length(s_dest) != length(s_arr)
            return false
        end
        if s_dest != s_arr
            return false
        end
    else
        # Ranges are 1D broadcast collections in this VM.
        if length(size(dest)) != 1
            return false
        end
    end

    f = bc.f
    if scalar_left
        for i in 1:n
            value = f(scalar, arr[i])
            dest[i] = value
        end
    else
        for i in 1:n
            value = f(arr[i], scalar)
            dest[i] = value
        end
    end
    return true
end

function _fastpath_2d_arg_compatible(arg, rows, cols)
    shape = _broadcastable_shape(arg)
    nd = length(shape)
    if nd == 0
        return true
    elseif nd == 1
        return shape[1] == 1 || shape[1] == rows
    elseif nd == 2
        d1 = shape[1]
        d2 = shape[2]
        return (d1 == 1 || d1 == rows) && (d2 == 1 || d2 == cols)
    else
        return false
    end
end

function _fastpath_arg_refs_dest_array(arg, dest)
    if isa(arg, Array)
        return arg === dest
    end
    if isa(arg, Broadcasted)
        bc_args = arg.bc_args
        n = length(bc_args)
        for i in 1:n
            if _fastpath_arg_refs_dest_array(bc_args[i], dest)
                return true
            end
        end
    end
    return false
end

# Fast path for 2D binary broadcasts.
# This avoids preprocess + CartesianIndex conversion when destination is 2D.
function _copyto_fastpath_2d_binary!(dest::Array, bc::Broadcasted)
    s = size(dest)
    if length(s) != 2
        return false
    end

    args = bc.bc_args
    if length(args) != 2
        return false
    end

    a = args[1]
    b = args[2]

    # A SubArray view is not a native `Array`; defer to the generic Extruded
    # loop so the view is indexed correctly rather than mis-classified (#5137).
    if isa(a, SubArray) || isa(b, SubArray)
        return false
    end

    # Preserve aliasing semantics for direct and nested Broadcasted args.
    if _fastpath_arg_refs_dest_array(a, dest) || _fastpath_arg_refs_dest_array(b, dest)
        return false
    end

    rows = s[1]
    cols = s[2]
    if !_fastpath_2d_arg_compatible(a, rows, cols) || !_fastpath_2d_arg_compatible(b, rows, cols)
        return false
    end

    f = bc.f
    linear = 1
    for j in 1:cols
        for i in 1:rows
            a_value = _getindex_one_2d(a, i, j)
            b_value = _getindex_one_2d(b, i, j)
            value = f(a_value, b_value)
            dest[linear] = value
            linear = linear + 1
        end
    end
    return true
end

# copyto! for Array from Broadcasted: the core broadcast loop
# For multi-dimensional broadcasts, we cannot use CartesianIndex objects because
# StructRef dispatch has limitations (DynamicToI64 conversion fails for CartesianIndex).
# Instead, we use specialized _broadcast_getindex_2d / _broadcast_getindex_3d helpers
# that pass individual dimension indices as integers (Issue #2686).
function copyto!(dest::Array, bc::Broadcasted)
    ibc = instantiate(bc)

    # Try optimized typed-array path before generic preprocessing/indexing.
    if _copyto_fastpath_same_shape_binary!(dest, ibc)
        return dest
    end
    if _copyto_fastpath_2d_binary!(dest, ibc)
        return dest
    end
    if _copyto_fastpath_array_scalar!(dest, ibc)
        return dest
    end

    # Preprocess: wrap arrays in Extruded, check aliasing
    bc_preprocessed = preprocess(dest, ibc)
    # Execute the broadcast loop
    n = length(dest)
    s = size(dest)
    nd = length(s)
    if nd <= 1
        # 1D: use linear indices directly (fast path)
        for i in 1:n
            value = _broadcast_getindex(bc_preprocessed, i)
            dest[i] = value
        end
    else
        # Multi-dimensional: convert linear index to CartesianIndex (Issue #2689)
        # This ensures Extruded arrays are indexed correctly per-dimension,
        # allowing proper broadcast dimension mapping (e.g., [3] .+ zeros(3,2)).
        for i in 1:n
            ci = _linear_to_cartesian(i, s)
            value = _broadcast_getindex(bc_preprocessed, ci)
            dest[i] = value
        end
    end
    return dest
end

# Convert 1-based linear index to CartesianIndex given array shape (Issue #2689)
# Uses column-major (Julia) ordering: first dimension varies fastest.
function _linear_to_cartesian(linear, shape)
    nd = length(shape)
    remaining = linear - 1  # convert to 0-based
    if nd == 2
        i1 = remaining % shape[1] + 1
        i2 = div(remaining, shape[1]) + 1
        return CartesianIndex((i1, i2))
    elseif nd == 3
        i1 = remaining % shape[1] + 1
        remaining = div(remaining, shape[1])
        i2 = remaining % shape[2] + 1
        i3 = div(remaining, shape[2]) + 1
        return CartesianIndex((i1, i2, i3))
    else
        # General case for nd >= 4
        i1 = remaining % shape[1] + 1
        remaining = div(remaining, shape[1])
        i2 = remaining % shape[2] + 1
        return CartesianIndex((i1, i2))
    end
end

# =============================================================================
# Phase 4-2: materialize / materialize! (Issue #2540)
# =============================================================================
# Based on Julia's base/broadcast.jl L893-905
#
# materialize converts a Broadcasted to an actual array.
# materialize! fills an existing array from a Broadcasted.

# materialize: lazy Broadcasted → Array
function materialize(bc::Broadcasted)
    return copy(instantiate(bc))
end

# materialize for non-Broadcasted: pass through
# INTENTIONAL_NOOP (Issue #4703): upstream `materialize(x) = x`
# (julia/base/broadcast.jl:900) is the generic pass-through fallback for
# any non-Broadcasted value; the typed `materialize(bc::Broadcasted)`
# above does the real work. A `return x` body is correct, not a stub.
function materialize(x)
    return x
end

# materialize!: in-place materialization
function materialize!(dest, bc::Broadcasted)
    style = bc.style
    f = bc.f
    args = bc.bc_args
    dest_axes = axes(dest)
    target = Broadcasted(style, f, args, dest_axes)
    ibc = instantiate(target)
    return copyto!(dest, ibc)
end

# materialize! for non-Broadcasted source: treat as identity broadcast
function materialize!(dest, x)
    return materialize!(dest, Broadcasted(identity, (x,)))
end

# =============================================================================
# Phase 1-2 (from coder-2): BroadcastStyle binary rules (Issue #2532)
# =============================================================================
# These supplement the workaround Phase 1-2 types above with proper
# dispatch-based combination rules from julia/base/broadcast.jl L128-220.

# Workaround: Official Julia uses DefaultArrayStyle{N} parametric struct.
# We define concrete types for each common dimensionality. (Issue #2523)
struct Unknown <: BroadcastStyle end
abstract type AbstractArrayStyle <: BroadcastStyle end
struct DefaultArrayStyle0 <: AbstractArrayStyle end
struct DefaultArrayStyle1 <: AbstractArrayStyle end
struct DefaultArrayStyle2 <: AbstractArrayStyle end
struct ArrayConflict <: AbstractArrayStyle end

# Fallback: two different styles -> Unknown
function broadcaststyle_combine(s1::BroadcastStyle, s2::BroadcastStyle)
    return Unknown()
end

function broadcaststyle_combine(s1::Unknown, s2::Unknown)
    return Unknown()
end

function broadcaststyle_combine(s1::DefaultArrayStyle0, s2::DefaultArrayStyle0)
    return DefaultArrayStyle0()
end

function broadcaststyle_combine(s1::DefaultArrayStyle1, s2::DefaultArrayStyle1)
    return DefaultArrayStyle1()
end

function broadcaststyle_combine(s1::DefaultArrayStyle2, s2::DefaultArrayStyle2)
    return DefaultArrayStyle2()
end

function broadcaststyle_combine(s1::BroadcastStyle, s2::Unknown)
    return s1
end

function broadcaststyle_combine(s1::Unknown, s2::BroadcastStyle)
    return s2
end

function broadcaststyle_combine(s1::DefaultArrayStyle0, s2::DefaultArrayStyle1)
    return DefaultArrayStyle1()
end

function broadcaststyle_combine(s1::DefaultArrayStyle1, s2::DefaultArrayStyle0)
    return DefaultArrayStyle1()
end

function broadcaststyle_combine(s1::DefaultArrayStyle0, s2::DefaultArrayStyle2)
    return DefaultArrayStyle2()
end

function broadcaststyle_combine(s1::DefaultArrayStyle2, s2::DefaultArrayStyle0)
    return DefaultArrayStyle2()
end

function broadcaststyle_combine(s1::DefaultArrayStyle1, s2::DefaultArrayStyle2)
    return DefaultArrayStyle2()
end

function broadcaststyle_combine(s1::DefaultArrayStyle2, s2::DefaultArrayStyle1)
    return DefaultArrayStyle2()
end

# Phase 1-3: combine_styles / result_style / result_join (Issue #2533)
function result_style(s::BroadcastStyle)
    return s
end

function result_style(s1::BroadcastStyle, s2::BroadcastStyle)
    return result_join(s1, s2, broadcaststyle_combine(s1, s2), broadcaststyle_combine(s2, s1))
end

function result_join(s1, s2, combined1::Unknown, combined2::Unknown)
    return ArrayConflict()
end

function result_join(s1, s2, combined1::Unknown, combined2::BroadcastStyle)
    return combined2
end

function result_join(s1, s2, combined1::BroadcastStyle, combined2::Unknown)
    return combined1
end

function result_join(s1, s2, combined1::BroadcastStyle, combined2::BroadcastStyle)
    return combined1
end

function combine_styles()
    return DefaultArrayStyle0()
end

function combine_styles(c)
    return DefaultArrayStyle0()
end

function combine_styles(c1, c2)
    return result_style(combine_styles(c1), combine_styles(c2))
end

# Phase 2-1: broadcast_shape / _bcs / _bcs1 (Issue #2535)
function _bcs1(a::Integer, b::Integer)
    if a == 1
        return b
    elseif b == 1
        return a
    elseif a == b
        return a
    else
        throw(DimensionMismatch("arrays could not be broadcast to a common size; got a dimension with lengths $a and $b"))
    end
end

function _prepend_to_tuple(val, t::Tuple)
    n = length(t)
    if n == 0
        return (val,)
    elseif n == 1
        return (val, t[1])
    elseif n == 2
        return (val, t[1], t[2])
    elseif n == 3
        return (val, t[1], t[2], t[3])
    elseif n == 4
        return (val, t[1], t[2], t[3], t[4])
    elseif n == 5
        return (val, t[1], t[2], t[3], t[4], t[5])
    elseif n == 6
        return (val, t[1], t[2], t[3], t[4], t[5], t[6])
    elseif n == 7
        return (val, t[1], t[2], t[3], t[4], t[5], t[6], t[7])
    else
        throw(DimensionMismatch("broadcast shape exceeds maximum supported dimensions (8)"))
    end
end

function _bcs(shape::Tuple, newshape::Tuple)
    n1 = length(shape)
    n2 = length(newshape)
    if n1 == 0 && n2 == 0
        return ()
    elseif n1 == 0
        return newshape
    elseif n2 == 0
        return shape
    else
        first_dim = _bcs1(shape[1], newshape[1])
        rest = _bcs(tail(shape), tail(newshape))
        return _prepend_to_tuple(first_dim, rest)
    end
end

function broadcast_shape(shape::Tuple)
    return shape
end

function broadcast_shape(shape::Tuple, shape1::Tuple)
    return _bcs(shape, shape1)
end

function broadcast_shape(shape::Tuple, shape1::Tuple, shape2::Tuple)
    return broadcast_shape(_bcs(shape, shape1), shape2)
end

function broadcast_shape(shape::Tuple, shape1::Tuple, shape2::Tuple, shape3::Tuple)
    return broadcast_shape(_bcs(shape, shape1), shape2, shape3)
end

# Phase 2-2: check_broadcast_shape (Issue #2536)
function _bcsm(a, b)
    return a == b || b == 1
end

function check_broadcast_shape(shp::Tuple)
    return nothing
end

function check_broadcast_shape(shp::Tuple, Ashp::Tuple)
    n_shp = length(shp)
    n_Ashp = length(Ashp)

    if n_Ashp == 0
        return nothing
    end

    if n_shp == 0
        for i in 1:n_Ashp
            if Ashp[i] != 1
                throw(DimensionMismatch("cannot broadcast array to have fewer non-singleton dimensions"))
            end
        end
        return nothing
    end

    if !_bcsm(shp[1], Ashp[1])
        throw(DimensionMismatch("array could not be broadcast to match destination"))
    end

    if n_shp > 1 && n_Ashp > 1
        check_broadcast_shape(tail(shp), tail(Ashp))
    elseif n_Ashp > 1
        remaining = tail(Ashp)
        for i in 1:length(remaining)
            if remaining[i] != 1
                throw(DimensionMismatch("cannot broadcast array to have fewer non-singleton dimensions"))
            end
        end
    end

    return nothing
end

# =============================================================================
# Phase 5-2: AndAnd / OrOr (Issue #2545)
# =============================================================================
# Short-circuit broadcast operators.
# Reference: julia/base/broadcast.jl L194-211
#
# In official Julia, AndAnd and OrOr are callable structs:
#   struct AndAnd end
#   (::AndAnd)(a, b) = a && b
#   const andand = AndAnd()
#
# Now supported via callable struct syntax (Issue #2671 fixed).

struct AndAnd end
struct OrOr end

# Callable struct methods (matching official Julia syntax)
(::AndAnd)(a, b) = a && b
(::OrOr)(a, b) = a || b

# Plain function aliases — the lowering maps .&& → andand and .|| → oror
# These are needed because the broadcast lowering emits calls to andand/oror
# by name, not via callable struct instances.
function andand(a, b)
    return a && b
end

function oror(a, b)
    return a || b
end

# =============================================================================
# Phase 5-1: flatten / isflat (Issue #2544)
# =============================================================================
# Loop fusion foundation. Flattens nested Broadcasted objects into a single
# level so that f.(g.(x)) becomes a single fused loop.
# Reference: julia/base/broadcast.jl L324-407

# Workaround: Tuple{} dispatch not supported by parser (Issue #2546)
# Using runtime length checks instead of parametric Tuple dispatch.
function isflat(bc::Broadcasted)
    return _isflat_rt(bc.bc_args)
end

function _isflat_rt(args)
    n = length(args)
    if n == 0
        return true
    end
    # Check that no argument is a Broadcasted (i.e., already flat)
    for i in 1:n
        if isa(args[i], Broadcasted)
            return false
        end
    end
    return true
end

# --- cat_nested helpers ---
# _tuple_cat: concatenate two tuples (fixed-arity, up to 6 total elements)
function _tuple_cat(t1, t2)
    n1 = length(t1)
    n2 = length(t2)
    if n1 == 0
        return t2
    elseif n2 == 0
        return t1
    elseif n1 == 1 && n2 == 1
        return (t1[1], t2[1])
    elseif n1 == 1 && n2 == 2
        return (t1[1], t2[1], t2[2])
    elseif n1 == 2 && n2 == 1
        return (t1[1], t1[2], t2[1])
    elseif n1 == 2 && n2 == 2
        return (t1[1], t1[2], t2[1], t2[2])
    elseif n1 == 1 && n2 == 3
        return (t1[1], t2[1], t2[2], t2[3])
    elseif n1 == 3 && n2 == 1
        return (t1[1], t1[2], t1[3], t2[1])
    elseif n1 == 2 && n2 == 3
        return (t1[1], t1[2], t2[1], t2[2], t2[3])
    elseif n1 == 3 && n2 == 2
        return (t1[1], t1[2], t1[3], t2[1], t2[2])
    elseif n1 == 3 && n2 == 3
        return (t1[1], t1[2], t1[3], t2[1], t2[2], t2[3])
    elseif n1 == 1 && n2 == 4
        return (t1[1], t2[1], t2[2], t2[3], t2[4])
    elseif n1 == 4 && n2 == 1
        return (t1[1], t1[2], t1[3], t1[4], t2[1])
    elseif n1 == 2 && n2 == 4
        return (t1[1], t1[2], t2[1], t2[2], t2[3], t2[4])
    elseif n1 == 4 && n2 == 2
        return (t1[1], t1[2], t1[3], t1[4], t2[1], t2[2])
    else
        # Fallback: return first tuple (should not happen for supported cases)
        return t1
    end
end

# _cat_one: if arg is Broadcasted, recurse into its args; otherwise wrap as 1-tuple
function _cat_one(arg)
    if isa(arg, Broadcasted)
        return _cat_nested_collect(arg.bc_args)
    else
        return (arg,)
    end
end

# _cat_nested_collect: recursively collect all leaf args from a tuple of args
function _cat_nested_collect(args)
    n = length(args)
    if n == 0
        return ()
    elseif n == 1
        return _cat_one(args[1])
    elseif n == 2
        return _tuple_cat(_cat_one(args[1]), _cat_one(args[2]))
    elseif n == 3
        t12 = _tuple_cat(_cat_one(args[1]), _cat_one(args[2]))
        return _tuple_cat(t12, _cat_one(args[3]))
    elseif n == 4
        t12 = _tuple_cat(_cat_one(args[1]), _cat_one(args[2]))
        t34 = _tuple_cat(_cat_one(args[3]), _cat_one(args[4]))
        return _tuple_cat(t12, t34)
    else
        return _cat_one(args[1])
    end
end

function cat_nested(bc)
    return _cat_nested_collect(bc.bc_args)
end

# --- make_makeargs helpers ---
# _count_leaves: count number of leaf (non-Broadcasted) args recursively
function _count_leaves(arg)
    if isa(arg, Broadcasted)
        total = 0
        n = length(arg.bc_args)
        for i in 1:n
            total = total + _count_leaves(arg.bc_args[i])
        end
        return total
    else
        return 1
    end
end

# Closure-based argument selectors (replacement for Pick{N} callable struct)
# Note: callable struct syntax is now supported (Issue #2671 fixed), but Pick{N}
# requires parametric callable structs which are not yet implemented.
# Using closure-based approach instead (simpler and sufficient for fusion).
# Workaround: Captured variables cannot be used as direct call targets in
# SubsetJuliaVM closures. Use _broadcast_apply as trampoline instead.
function _make_leaf_selector(idx)
    function sel(flat_args)
        return flat_args[idx]
    end
    return sel
end

function _make_bc1_selector(inner_f, idx)
    function sel(flat_args)
        return _broadcast_apply(inner_f, (flat_args[idx],))
    end
    return sel
end

function _make_bc2_selector(inner_f, idx)
    function sel(flat_args)
        return _broadcast_apply(inner_f, (flat_args[idx], flat_args[idx + 1]))
    end
    return sel
end

function _make_bc3_selector(inner_f, idx)
    function sel(flat_args)
        return _broadcast_apply(inner_f, (flat_args[idx], flat_args[idx + 1], flat_args[idx + 2]))
    end
    return sel
end

function _make_arg_selector(arg, offset)
    if isa(arg, Broadcasted)
        inner_n = length(arg.bc_args)
        if inner_n == 1
            return _make_bc1_selector(arg.f, offset)
        elseif inner_n == 2
            return _make_bc2_selector(arg.f, offset)
        elseif inner_n == 3
            return _make_bc3_selector(arg.f, offset)
        end
    end
    return _make_leaf_selector(offset)
end

# make_makeargs: create tuple of closure-based argument selectors
# Each selector picks the right flat args and applies inner functions if needed
function make_makeargs(bc_args)
    n = length(bc_args)
    if n == 0
        return ()
    end
    offset = 1
    if n == 1
        return (_make_arg_selector(bc_args[1], offset),)
    elseif n == 2
        n1 = _count_leaves(bc_args[1])
        sel1 = _make_arg_selector(bc_args[1], offset)
        sel2 = _make_arg_selector(bc_args[2], offset + n1)
        return (sel1, sel2)
    elseif n == 3
        n1 = _count_leaves(bc_args[1])
        n2 = _count_leaves(bc_args[2])
        sel1 = _make_arg_selector(bc_args[1], offset)
        sel2 = _make_arg_selector(bc_args[2], offset + n1)
        sel3 = _make_arg_selector(bc_args[3], offset + n1 + n2)
        return (sel1, sel2, sel3)
    elseif n == 4
        n1 = _count_leaves(bc_args[1])
        n2 = _count_leaves(bc_args[2])
        n3 = _count_leaves(bc_args[3])
        sel1 = _make_arg_selector(bc_args[1], offset)
        sel2 = _make_arg_selector(bc_args[2], offset + n1)
        sel3 = _make_arg_selector(bc_args[3], offset + n1 + n2)
        sel4 = _make_arg_selector(bc_args[4], offset + n1 + n2 + n3)
        return (sel1, sel2, sel3, sel4)
    end
    return ()
end

# --- Fusion helper functions ---
# Each returns a closure that captures inner function(s) and computes the fused result.
# Used by flatten() to create single-level fused Broadcasted functions.
# Workaround: Captured variables cannot be used as direct call targets in
# SubsetJuliaVM closures, so we use _broadcast_apply as trampoline.

# f(g(x)) — unary outer, unary inner
function _make_fused_f_gx(outer_f, inner_g)
    function fused(x)
        inner_result = _broadcast_apply(inner_g, (x,))
        return _broadcast_apply(outer_f, (inner_result,))
    end
    return fused
end

# f(g(x,y)) — unary outer, binary inner
function _make_fused_f_gxy(outer_f, inner_g)
    function fused(x, y)
        inner_result = _broadcast_apply(inner_g, (x, y))
        return _broadcast_apply(outer_f, (inner_result,))
    end
    return fused
end

# f(g(x,y,z)) — unary outer, ternary inner
function _make_fused_f_gxyz(outer_f, inner_g)
    function fused(x, y, z)
        inner_result = _broadcast_apply(inner_g, (x, y, z))
        return _broadcast_apply(outer_f, (inner_result,))
    end
    return fused
end

# f(g(x), y) — binary outer, first arg from unary inner
function _make_fused_fgx_y(outer_f, inner_g)
    function fused(x, y)
        g_result = _broadcast_apply(inner_g, (x,))
        return _broadcast_apply(outer_f, (g_result, y))
    end
    return fused
end

# f(g(x,y), z) — binary outer, first arg from binary inner
function _make_fused_fgxy_z(outer_f, inner_g)
    function fused(x, y, z)
        g_result = _broadcast_apply(inner_g, (x, y))
        return _broadcast_apply(outer_f, (g_result, z))
    end
    return fused
end

# f(x, g(y)) — binary outer, second arg from unary inner
function _make_fused_fx_gy(outer_f, inner_g)
    function fused(x, y)
        g_result = _broadcast_apply(inner_g, (y,))
        return _broadcast_apply(outer_f, (x, g_result))
    end
    return fused
end

# f(x, g(y,z)) — binary outer, second arg from binary inner
function _make_fused_fx_gyz(outer_f, inner_g)
    function fused(x, y, z)
        g_result = _broadcast_apply(inner_g, (y, z))
        return _broadcast_apply(outer_f, (x, g_result))
    end
    return fused
end

# f(g(x), h(y)) — binary outer, both from unary inners
function _make_fused_fgx_hy(outer_f, inner_g, inner_h)
    function fused(x, y)
        g_result = _broadcast_apply(inner_g, (x,))
        h_result = _broadcast_apply(inner_h, (y,))
        return _broadcast_apply(outer_f, (g_result, h_result))
    end
    return fused
end

# f(g(x,y), h(z)) — binary outer, first from binary, second from unary
function _make_fused_fgxy_hz(outer_f, inner_g, inner_h)
    function fused(x, y, z)
        g_result = _broadcast_apply(inner_g, (x, y))
        h_result = _broadcast_apply(inner_h, (z,))
        return _broadcast_apply(outer_f, (g_result, h_result))
    end
    return fused
end

# f(g(x), h(y,z)) — binary outer, first from unary, second from binary
function _make_fused_fgx_hyz(outer_f, inner_g, inner_h)
    function fused(x, y, z)
        g_result = _broadcast_apply(inner_g, (x,))
        h_result = _broadcast_apply(inner_h, (y, z))
        return _broadcast_apply(outer_f, (g_result, h_result))
    end
    return fused
end

# f(g(x,y), h(z,w)) — binary outer, both from binary inners (Issue #2679)
function _make_fused_fgxy_hzw(outer_f, inner_g, inner_h)
    function fused(x, y, z, w)
        g_result = _broadcast_apply(inner_g, (x, y))
        h_result = _broadcast_apply(inner_h, (z, w))
        return _broadcast_apply(outer_f, (g_result, h_result))
    end
    return fused
end

# f(g(x,y,z), w) — binary outer, first from ternary inner, second leaf (Issue #2679)
function _make_fused_fgxyz_w(outer_f, inner_g)
    function fused(x, y, z, w)
        g_result = _broadcast_apply(inner_g, (x, y, z))
        return _broadcast_apply(outer_f, (g_result, w))
    end
    return fused
end

# f(w, g(x,y,z)) — binary outer, first leaf, second from ternary inner (Issue #2679)
function _make_fused_fw_gxyz(outer_f, inner_g)
    function fused(w, x, y, z)
        g_result = _broadcast_apply(inner_g, (x, y, z))
        return _broadcast_apply(outer_f, (w, g_result))
    end
    return fused
end

# f(g(x,y,z), h(w)) — binary outer, first from ternary, second from unary (Issue #2679)
function _make_fused_fgxyz_hw(outer_f, inner_g, inner_h)
    function fused(x, y, z, w)
        g_result = _broadcast_apply(inner_g, (x, y, z))
        h_result = _broadcast_apply(inner_h, (w,))
        return _broadcast_apply(outer_f, (g_result, h_result))
    end
    return fused
end

# f(g(x), h(y,z,w)) — binary outer, first from unary, second from ternary (Issue #2679)
function _make_fused_fgx_hyzw(outer_f, inner_g, inner_h)
    function fused(x, y, z, w)
        g_result = _broadcast_apply(inner_g, (x,))
        h_result = _broadcast_apply(inner_h, (y, z, w))
        return _broadcast_apply(outer_f, (g_result, h_result))
    end
    return fused
end

# f(g(x,y,z,w)) — unary outer, quaternary inner (Issue #2679)
function _make_fused_f_gxyzw(outer_f, inner_g)
    function fused(x, y, z, w)
        inner_result = _broadcast_apply(inner_g, (x, y, z, w))
        return _broadcast_apply(outer_f, (inner_result,))
    end
    return fused
end

# flatten: flatten nested Broadcasted into a single level with fused function
# Reference: julia/base/broadcast.jl L324-407
# Note: Uses closure-based fusion instead of Pick{N} callable structs.
# Callable struct syntax is now supported (Issue #2671 fixed), but Pick{N}
# requires parametric callable structs which are not yet implemented.
function flatten(bc::Broadcasted)
    isflat(bc) && return bc

    bc_args = bc.bc_args
    n = length(bc_args)
    f = bc.f

    if n == 1
        arg1 = bc_args[1]
        if isa(arg1, Broadcasted)
            # Recursively flatten inner Broadcasted first
            flat_inner = flatten(arg1)
            inner_args = flat_inner.bc_args
            ni = length(inner_args)
            if ni == 1
                new_f = _make_fused_f_gx(f, flat_inner.f)
                return Broadcasted(bc.style, new_f, inner_args, bc.axes_val)
            elseif ni == 2
                new_f = _make_fused_f_gxy(f, flat_inner.f)
                return Broadcasted(bc.style, new_f, inner_args, bc.axes_val)
            elseif ni == 3
                new_f = _make_fused_f_gxyz(f, flat_inner.f)
                return Broadcasted(bc.style, new_f, inner_args, bc.axes_val)
            elseif ni == 4
                # Issue #2679: support 4-arg flattened inner
                new_f = _make_fused_f_gxyzw(f, flat_inner.f)
                return Broadcasted(bc.style, new_f, inner_args, bc.axes_val)
            end
        end
    elseif n == 2
        arg1 = bc_args[1]
        arg2 = bc_args[2]
        is_bc1 = isa(arg1, Broadcasted)
        is_bc2 = isa(arg2, Broadcasted)

        if is_bc1 && !is_bc2
            flat1 = flatten(arg1)
            g_args = flat1.bc_args
            ng = length(g_args)
            if ng == 1
                new_f = _make_fused_fgx_y(f, flat1.f)
                flat_args = (g_args[1], arg2)
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif ng == 2
                new_f = _make_fused_fgxy_z(f, flat1.f)
                flat_args = (g_args[1], g_args[2], arg2)
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif ng == 3
                # Issue #2679: f(g(x,y,z), w) — first from ternary, second leaf
                new_f = _make_fused_fgxyz_w(f, flat1.f)
                flat_args = (g_args[1], g_args[2], g_args[3], arg2)
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            end
        elseif !is_bc1 && is_bc2
            flat2 = flatten(arg2)
            h_args = flat2.bc_args
            nh = length(h_args)
            if nh == 1
                new_f = _make_fused_fx_gy(f, flat2.f)
                flat_args = (arg1, h_args[1])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif nh == 2
                new_f = _make_fused_fx_gyz(f, flat2.f)
                flat_args = (arg1, h_args[1], h_args[2])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif nh == 3
                # Issue #2679: f(w, g(x,y,z)) — first leaf, second from ternary
                new_f = _make_fused_fw_gxyz(f, flat2.f)
                flat_args = (arg1, h_args[1], h_args[2], h_args[3])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            end
        elseif is_bc1 && is_bc2
            flat1 = flatten(arg1)
            flat2 = flatten(arg2)
            g_args = flat1.bc_args
            h_args = flat2.bc_args
            ng = length(g_args)
            nh = length(h_args)
            if ng == 1 && nh == 1
                new_f = _make_fused_fgx_hy(f, flat1.f, flat2.f)
                flat_args = (g_args[1], h_args[1])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif ng == 2 && nh == 1
                new_f = _make_fused_fgxy_hz(f, flat1.f, flat2.f)
                flat_args = (g_args[1], g_args[2], h_args[1])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif ng == 1 && nh == 2
                new_f = _make_fused_fgx_hyz(f, flat1.f, flat2.f)
                flat_args = (g_args[1], h_args[1], h_args[2])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif ng == 2 && nh == 2
                # Issue #2679: f(g(x,y), h(z,w)) — both binary inners
                new_f = _make_fused_fgxy_hzw(f, flat1.f, flat2.f)
                flat_args = (g_args[1], g_args[2], h_args[1], h_args[2])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif ng == 3 && nh == 1
                # Issue #2679: f(g(x,y,z), h(w)) — first ternary, second unary
                new_f = _make_fused_fgxyz_hw(f, flat1.f, flat2.f)
                flat_args = (g_args[1], g_args[2], g_args[3], h_args[1])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            elseif ng == 1 && nh == 3
                # Issue #2679: f(g(x), h(y,z,w)) — first unary, second ternary
                new_f = _make_fused_fgx_hyzw(f, flat1.f, flat2.f)
                flat_args = (g_args[1], h_args[1], h_args[2], h_args[3])
                return Broadcasted(bc.style, new_f, flat_args, bc.axes_val)
            end
        end
    end

    # Fallback: return as-is (unsupported nesting pattern)
    return bc
end

# =============================================================================
# Phase 6-3: broadcast / broadcast! entry points (Issue #2548)
# =============================================================================
# Based on Julia's base/broadcast.jl L794-886
#
# These entry points convert broadcast(f, As...) calls into the Broadcasted
# pipeline: broadcast(f, As...) = materialize(broadcasted(f, As...))

# broadcasted: create a lazy Broadcasted wrapper
# Based on Julia's base/broadcast.jl L794-829
function broadcasted(f, A)
    return Broadcasted(nothing, f, (A,))
end

function broadcasted(f, A, B)
    return Broadcasted(nothing, f, (A, B))
end

function broadcasted(f, A, B, C)
    return Broadcasted(nothing, f, (A, B, C))
end

function broadcasted(f, A, B, C, D)
    return Broadcasted(nothing, f, (A, B, C, D))
end

# broadcast: eager entry point — materialize a lazy Broadcasted wrapper
# Based on Julia's base/broadcast.jl L836-886 (Issue #2548, #2549)
function broadcast(f, A)
    bc = broadcasted(f, A)
    return materialize(bc)
end
broadcast(::typeof(identity), A::Vector{Int64}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{Int8}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{Int16}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{Int32}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{UInt8}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{UInt16}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{UInt32}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{UInt64}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{Float64}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{Float32}) = map(identity, A)
broadcast(::typeof(identity), A::Vector{Bool}) = _mark_bitvector(map(identity, A))
broadcast(::typeof(abs), A::Vector{Int64}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{Int8}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{Int16}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{Int32}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{UInt8}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{UInt16}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{UInt32}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{UInt64}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{Float64}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{Float32}) = map(abs, A)
broadcast(::typeof(abs), A::Vector{Bool}) = _mark_bitvector(map(abs, A))
broadcast(::typeof(abs2), A::Vector{Int64}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{Int8}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{Int16}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{Int32}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{UInt8}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{UInt16}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{UInt32}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{UInt64}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{Float64}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{Float32}) = map(abs2, A)
broadcast(::typeof(abs2), A::Vector{Bool}) = _mark_bitvector(map(abs2, A))
broadcast(::typeof(-), A::Vector{Int64}) = map(-, A)
broadcast(::typeof(-), A::Vector{Int8}) = map(-, A)
broadcast(::typeof(-), A::Vector{Int16}) = map(-, A)
broadcast(::typeof(-), A::Vector{Int32}) = map(-, A)
broadcast(::typeof(-), A::Vector{UInt8}) = map(-, A)
broadcast(::typeof(-), A::Vector{UInt16}) = map(-, A)
broadcast(::typeof(-), A::Vector{UInt32}) = map(-, A)
broadcast(::typeof(-), A::Vector{UInt64}) = map(-, A)
broadcast(::typeof(-), A::Vector{Float64}) = map(-, A)
broadcast(::typeof(-), A::Vector{Float32}) = map(-, A)
_broadcast_bool_unary(f, A) = _mark_bitvector(_map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), f, A))
broadcast(::typeof(iszero), A::Vector{Int64}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{Int8}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{Int16}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{Int32}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{UInt8}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{UInt16}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{UInt32}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{UInt64}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{Float64}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{Float32}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(iszero), A::Vector{Bool}) = _broadcast_bool_unary(iszero, A)
broadcast(::typeof(isone), A::Vector{Int64}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{Int8}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{Int16}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{Int32}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{UInt8}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{UInt16}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{UInt32}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{UInt64}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{Float64}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{Float32}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(isone), A::Vector{Bool}) = _broadcast_bool_unary(isone, A)
broadcast(::typeof(signbit), A::Vector{Int64}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{Int8}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{Int16}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{Int32}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{UInt8}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{UInt16}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{UInt32}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{UInt64}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{Float64}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{Float32}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(signbit), A::Vector{Bool}) = _broadcast_bool_unary(signbit, A)
broadcast(::typeof(iseven), A::Vector{Int64}) = _broadcast_bool_unary(iseven, A)
broadcast(::typeof(iseven), A::Vector{Int8}) = _broadcast_bool_unary(iseven, A)
broadcast(::typeof(iseven), A::Vector{Int16}) = _broadcast_bool_unary(iseven, A)
broadcast(::typeof(iseven), A::Vector{Int32}) = _broadcast_bool_unary(iseven, A)
broadcast(::typeof(iseven), A::Vector{UInt8}) = _broadcast_bool_unary(iseven, A)
broadcast(::typeof(iseven), A::Vector{UInt16}) = _broadcast_bool_unary(iseven, A)
broadcast(::typeof(iseven), A::Vector{UInt32}) = _broadcast_bool_unary(iseven, A)
broadcast(::typeof(iseven), A::Vector{UInt64}) = _broadcast_bool_unary(iseven, A)
broadcast(::typeof(isodd), A::Vector{Int64}) = _broadcast_bool_unary(isodd, A)
broadcast(::typeof(isodd), A::Vector{Int8}) = _broadcast_bool_unary(isodd, A)
broadcast(::typeof(isodd), A::Vector{Int16}) = _broadcast_bool_unary(isodd, A)
broadcast(::typeof(isodd), A::Vector{Int32}) = _broadcast_bool_unary(isodd, A)
broadcast(::typeof(isodd), A::Vector{UInt8}) = _broadcast_bool_unary(isodd, A)
broadcast(::typeof(isodd), A::Vector{UInt16}) = _broadcast_bool_unary(isodd, A)
broadcast(::typeof(isodd), A::Vector{UInt32}) = _broadcast_bool_unary(isodd, A)
broadcast(::typeof(isodd), A::Vector{UInt64}) = _broadcast_bool_unary(isodd, A)
function broadcast(f, A, B)
    bc = broadcasted(f, A, B)
    return materialize(bc)
end
function _broadcast_same_length_binary_map(f, A, B)
    if length(A) == length(B)
        return map(f, A, B)
    end
    bc = broadcasted(f, A, B)
    return materialize(bc)
end
function _broadcast_same_length_ternary_map(f, A, B, C)
    if length(A) == length(B) && length(A) == length(C)
        return map(f, A, B, C)
    end
    bc = broadcasted(f, A, B, C)
    return materialize(bc)
end
function _broadcast_same_length_quaternary_map(f, A, B, C, D)
    if length(A) == length(B) && length(A) == length(C) && length(A) == length(D)
        return map(f, A, B, C, D)
    end
    bc = broadcasted(f, A, B, C, D)
    return materialize(bc)
end
function _broadcast_vector_vararg_length(A, B, C, As)
    n = _bcs1(length(A), length(B))
    n = _bcs1(n, length(C))
    for j in 1:length(As)
        n = _bcs1(n, length(As[j]))
    end
    return n
end
function _broadcast_vector_arg(A, i)
    if length(A) == 1
        return A[1]
    end
    return A[i]
end
function _broadcast_vector_vararg_plus_into!(result, A, B, C, As)
    n = length(result)
    for i in 1:n
        value = _broadcast_vector_arg(A, i) + _broadcast_vector_arg(B, i)
        value = value + _broadcast_vector_arg(C, i)
        for j in 1:length(As)
            value = value + _broadcast_vector_arg(As[j], i)
        end
        result[i] = value
    end
    return result
end
function _broadcast_vector_vararg_plus_similar(A, B, C, As)
    n = _broadcast_vector_vararg_length(A, B, C, As)
    return _broadcast_vector_vararg_plus_into!(similar(A, n), A, B, C, As)
end
function _broadcast_vector_vararg_mul_into!(result, A, B, C, As)
    n = length(result)
    for i in 1:n
        value = _broadcast_vector_arg(A, i) * _broadcast_vector_arg(B, i)
        value = value * _broadcast_vector_arg(C, i)
        for j in 1:length(As)
            value = value * _broadcast_vector_arg(As[j], i)
        end
        result[i] = value
    end
    return result
end
function _broadcast_vector_vararg_mul_similar(A, B, C, As)
    n = _broadcast_vector_vararg_length(A, B, C, As)
    return _broadcast_vector_vararg_mul_into!(similar(A, n), A, B, C, As)
end
function _broadcast_vector_vararg_min_into!(result, A, B, C, As)
    n = length(result)
    for i in 1:n
        value = min(_broadcast_vector_arg(A, i), _broadcast_vector_arg(B, i))
        value = min(value, _broadcast_vector_arg(C, i))
        for j in 1:length(As)
            value = min(value, _broadcast_vector_arg(As[j], i))
        end
        result[i] = value
    end
    return result
end
function _broadcast_vector_vararg_min_similar(A, B, C, As)
    n = _broadcast_vector_vararg_length(A, B, C, As)
    return _broadcast_vector_vararg_min_into!(similar(A, n), A, B, C, As)
end
function _broadcast_vector_vararg_max_into!(result, A, B, C, As)
    n = length(result)
    for i in 1:n
        value = max(_broadcast_vector_arg(A, i), _broadcast_vector_arg(B, i))
        value = max(value, _broadcast_vector_arg(C, i))
        for j in 1:length(As)
            value = max(value, _broadcast_vector_arg(As[j], i))
        end
        result[i] = value
    end
    return result
end
function _broadcast_vector_vararg_max_similar(A, B, C, As)
    n = _broadcast_vector_vararg_length(A, B, C, As)
    return _broadcast_vector_vararg_max_into!(similar(A, n), A, B, C, As)
end
broadcast(::typeof(+), A::Vector{Int64}, B::Vector{Int64}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{Int8}, B::Vector{Int8}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{Int16}, B::Vector{Int16}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{Int32}, B::Vector{Int32}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{UInt8}, B::Vector{UInt8}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{UInt16}, B::Vector{UInt16}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{UInt32}, B::Vector{UInt32}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{UInt64}, B::Vector{UInt64}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{Float32}, B::Vector{Float32}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{Float64}, B::Vector{Float64}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{Bool}, B::Vector{Bool}) = _broadcast_same_length_binary_map(+, A, B)
broadcast(::typeof(+), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}) = _broadcast_same_length_ternary_map(+, A, B, C)
broadcast(::typeof(+), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}) = _broadcast_same_length_ternary_map(+, A, B, C)
broadcast(::typeof(+), A::Vector{Bool}, B::Vector{Bool}, C::Vector{Bool}) = _broadcast_same_length_ternary_map(+, A, B, C)
broadcast(::typeof(+), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, D::Vector{Int32}) = _broadcast_same_length_quaternary_map(+, A, B, C, D)
broadcast(::typeof(+), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, D::Vector{Float32}) = _broadcast_same_length_quaternary_map(+, A, B, C, D)
broadcast(::typeof(+), A::Vector{Int64}, B::Vector{Int64}, C::Vector{Int64}, As::Vector{Int64}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{Int8}, B::Vector{Int8}, C::Vector{Int8}, As::Vector{Int8}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{Int16}, B::Vector{Int16}, C::Vector{Int16}, As::Vector{Int16}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, As::Vector{Int32}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{UInt8}, B::Vector{UInt8}, C::Vector{UInt8}, As::Vector{UInt8}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{UInt16}, B::Vector{UInt16}, C::Vector{UInt16}, As::Vector{UInt16}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{UInt32}, B::Vector{UInt32}, C::Vector{UInt32}, As::Vector{UInt32}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{UInt64}, B::Vector{UInt64}, C::Vector{UInt64}, As::Vector{UInt64}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, As::Vector{Float32}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{Float64}, B::Vector{Float64}, C::Vector{Float64}, As::Vector{Float64}...) = _broadcast_vector_vararg_plus_similar(A, B, C, As)
broadcast(::typeof(+), A::Vector{Bool}, B::Vector{Bool}, C::Vector{Bool}, As::Vector{Bool}...) = _broadcast_vector_vararg_plus_into!(_array_undef_from_dims(Int64, (_broadcast_vector_vararg_length(A, B, C, As),)), A, B, C, As)
broadcast(::typeof(*), A::Vector{Int64}, B::Vector{Int64}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{Int8}, B::Vector{Int8}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{Int16}, B::Vector{Int16}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{Int32}, B::Vector{Int32}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{UInt8}, B::Vector{UInt8}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{UInt16}, B::Vector{UInt16}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{UInt32}, B::Vector{UInt32}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{UInt64}, B::Vector{UInt64}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{Float32}, B::Vector{Float32}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{Float64}, B::Vector{Float64}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(*), A::Vector{Bool}, B::Vector{Bool}) = _broadcast_same_length_binary_map(*, A, B)
broadcast(::typeof(min), A::Vector{Int64}, B::Vector{Int64}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{Int8}, B::Vector{Int8}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{Int16}, B::Vector{Int16}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{Int32}, B::Vector{Int32}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{UInt8}, B::Vector{UInt8}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{UInt16}, B::Vector{UInt16}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{UInt32}, B::Vector{UInt32}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{UInt64}, B::Vector{UInt64}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{Float32}, B::Vector{Float32}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(min), A::Vector{Float64}, B::Vector{Float64}) = _broadcast_same_length_binary_map(min, A, B)
broadcast(::typeof(max), A::Vector{Int64}, B::Vector{Int64}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{Int8}, B::Vector{Int8}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{Int16}, B::Vector{Int16}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{Int32}, B::Vector{Int32}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{UInt8}, B::Vector{UInt8}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{UInt16}, B::Vector{UInt16}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{UInt32}, B::Vector{UInt32}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{UInt64}, B::Vector{UInt64}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{Float32}, B::Vector{Float32}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(max), A::Vector{Float64}, B::Vector{Float64}) = _broadcast_same_length_binary_map(max, A, B)
broadcast(::typeof(*), A::Vector{Int64}, B::Vector{Int64}, C::Vector{Int64}, As::Vector{Int64}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{Int8}, B::Vector{Int8}, C::Vector{Int8}, As::Vector{Int8}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{Int16}, B::Vector{Int16}, C::Vector{Int16}, As::Vector{Int16}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, As::Vector{Int32}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{UInt8}, B::Vector{UInt8}, C::Vector{UInt8}, As::Vector{UInt8}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{UInt16}, B::Vector{UInt16}, C::Vector{UInt16}, As::Vector{UInt16}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{UInt32}, B::Vector{UInt32}, C::Vector{UInt32}, As::Vector{UInt32}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{UInt64}, B::Vector{UInt64}, C::Vector{UInt64}, As::Vector{UInt64}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, As::Vector{Float32}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{Float64}, B::Vector{Float64}, C::Vector{Float64}, As::Vector{Float64}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(*), A::Vector{Bool}, B::Vector{Bool}, C::Vector{Bool}, As::Vector{Bool}...) = _broadcast_vector_vararg_mul_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{Int64}, B::Vector{Int64}, C::Vector{Int64}, As::Vector{Int64}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{Int8}, B::Vector{Int8}, C::Vector{Int8}, As::Vector{Int8}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{Int16}, B::Vector{Int16}, C::Vector{Int16}, As::Vector{Int16}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, As::Vector{Int32}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{UInt8}, B::Vector{UInt8}, C::Vector{UInt8}, As::Vector{UInt8}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{UInt16}, B::Vector{UInt16}, C::Vector{UInt16}, As::Vector{UInt16}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{UInt32}, B::Vector{UInt32}, C::Vector{UInt32}, As::Vector{UInt32}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{UInt64}, B::Vector{UInt64}, C::Vector{UInt64}, As::Vector{UInt64}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, As::Vector{Float32}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(min), A::Vector{Float64}, B::Vector{Float64}, C::Vector{Float64}, As::Vector{Float64}...) = _broadcast_vector_vararg_min_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{Int64}, B::Vector{Int64}, C::Vector{Int64}, As::Vector{Int64}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{Int8}, B::Vector{Int8}, C::Vector{Int8}, As::Vector{Int8}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{Int16}, B::Vector{Int16}, C::Vector{Int16}, As::Vector{Int16}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, As::Vector{Int32}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{UInt8}, B::Vector{UInt8}, C::Vector{UInt8}, As::Vector{UInt8}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{UInt16}, B::Vector{UInt16}, C::Vector{UInt16}, As::Vector{UInt16}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{UInt32}, B::Vector{UInt32}, C::Vector{UInt32}, As::Vector{UInt32}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{UInt64}, B::Vector{UInt64}, C::Vector{UInt64}, As::Vector{UInt64}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, As::Vector{Float32}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(max), A::Vector{Float64}, B::Vector{Float64}, C::Vector{Float64}, As::Vector{Float64}...) = _broadcast_vector_vararg_max_similar(A, B, C, As)
broadcast(::typeof(-), A::Vector{Int64}, B::Vector{Int64}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{Int8}, B::Vector{Int8}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{Int16}, B::Vector{Int16}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{Int32}, B::Vector{Int32}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{UInt8}, B::Vector{UInt8}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{UInt16}, B::Vector{UInt16}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{UInt32}, B::Vector{UInt32}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{UInt64}, B::Vector{UInt64}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{Float32}, B::Vector{Float32}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(-), A::Vector{Float64}, B::Vector{Float64}) = _broadcast_same_length_binary_map(-, A, B)
broadcast(::typeof(/), A::Vector{Int64}, B::Vector{Int64}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{Int8}, B::Vector{Int8}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{Int16}, B::Vector{Int16}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{Int32}, B::Vector{Int32}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{UInt8}, B::Vector{UInt8}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{UInt16}, B::Vector{UInt16}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{UInt32}, B::Vector{UInt32}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{UInt64}, B::Vector{UInt64}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{Float32}, B::Vector{Float32}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{Float64}, B::Vector{Float64}) = _broadcast_same_length_binary_map(/, A, B)
broadcast(::typeof(/), A::Vector{Bool}, B::Vector{Bool}) = _broadcast_same_length_binary_map(/, A, B)
function broadcast(f, A, B, C)
    bc = broadcasted(f, A, B, C)
    return materialize(bc)
end
function broadcast(f, A, B, C, D)
    bc = broadcasted(f, A, B, C, D)
    return materialize(bc)
end
# Scalar-only optimizations: skip Broadcasted pipeline entirely
function broadcast(f, a::Number, b::Number)
    return f(a, b)
end
function broadcast(f, a::Number)
    return f(a)
end

# broadcast!: in-place entry point
# Based on Julia's base/broadcast.jl L856-886
function broadcast!(f, dest, A)
    bc = broadcasted(f, A)
    materialize!(dest, bc)
    return dest
end
function broadcast!(f, dest, A, B)
    bc = broadcasted(f, A, B)
    materialize!(dest, bc)
    return dest
end
function broadcast!(f, dest, A, B, C)
    bc = broadcasted(f, A, B, C)
    materialize!(dest, bc)
    return dest
end
function broadcast!(f, dest, A, B, C, D)
    bc = broadcasted(f, A, B, C, D)
    materialize!(dest, bc)
    return dest
end

# =============================================================================
# Phase 7-3: show / display methods (Issue #2551)
# =============================================================================
# Reference: julia/base/broadcast.jl L216-224

# show for BroadcastStyle subtypes
# Now using Base.show qualified names (Issue #2671 fixed).
# Note: Using named parameters instead of unnamed '::Type' because SubsetJuliaVM
# uses non-parametric DefaultArrayStyle (workaround Issue #2531).
# In official Julia: Base.show(io::IO, ::DefaultArrayStyle{N}) where N = print(io, "DefaultArrayStyle{$N}()")
Base.show(io::IO, s::DefaultArrayStyle) = print(io, "DefaultArrayStyle{", s.dims, "}()")
Base.show(io::IO, s::TupleBroadcastStyle) = print(io, "Style{Tuple}()")
Base.show(io::IO, s::BroadcastStyleUnknown) = print(io, "Unknown()")

# show for Broadcasted (Issue #2671 fixed: now using Base.show)
function Base.show(io::IO, bc::Broadcasted)
    print(io, "Broadcasted(")
    print(io, bc.f)
    print(io, ", ")
    show(io, bc.bc_args)
    print(io, ")")
end

# show for AndAnd/OrOr callable struct instances (Issue #2671 fixed)
Base.show(io::IO, ::AndAnd) = print(io, "andand")
Base.show(io::IO, ::OrOr) = print(io, "oror")
