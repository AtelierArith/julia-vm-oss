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
        result = result * length(ax[i])
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
    elseif isa(x, Tuple)
        return (length(x),)
    elseif isa(x, Broadcasted)
        ax = axes(x)
        n = length(ax)
        if n == 1
            return (length(ax[1]),)
        elseif n == 2
            return (length(ax[1]), length(ax[2]))
        elseif n == 3
            return (length(ax[1]), length(ax[2]), length(ax[3]))
        else
            return (length(ax[1]),)
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

# combine_axes: compute combined axes from broadcast arguments
function _broadcast_combine_axes(args)
    n = length(args)
    if n == 0
        return ()
    end
    shape = _broadcastable_shape(args[1])
    for i in 2:n
        shape = _broadcast_shape(shape, _broadcastable_shape(args[i]))
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
        return (_getindex_one(args[1], I),)
    elseif n == 2
        return (_getindex_one(args[1], I), _getindex_one(args[2], I))
    elseif n == 3
        return (_getindex_one(args[1], I), _getindex_one(args[2], I), _getindex_one(args[3], I))
    elseif n == 4
        return (_getindex_one(args[1], I), _getindex_one(args[2], I), _getindex_one(args[3], I), _getindex_one(args[4], I))
    else
        # Fallback: handle first 4 args
        return (_getindex_one(args[1], I), _getindex_one(args[2], I), _getindex_one(args[3], I), _getindex_one(args[4], I))
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
        return (_getindex_one_2d(args[1], i, j),)
    elseif n == 2
        return (_getindex_one_2d(args[1], i, j), _getindex_one_2d(args[2], i, j))
    elseif n == 3
        return (_getindex_one_2d(args[1], i, j), _getindex_one_2d(args[2], i, j), _getindex_one_2d(args[3], i, j))
    elseif n == 4
        return (_getindex_one_2d(args[1], i, j), _getindex_one_2d(args[2], i, j), _getindex_one_2d(args[3], i, j), _getindex_one_2d(args[4], i, j))
    else
        return (_getindex_one_2d(args[1], i, j), _getindex_one_2d(args[2], i, j), _getindex_one_2d(args[3], i, j), _getindex_one_2d(args[4], i, j))
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
        return (_getindex_one_3d(args[1], i, j, k),)
    elseif n == 2
        return (_getindex_one_3d(args[1], i, j, k), _getindex_one_3d(args[2], i, j, k))
    elseif n == 3
        return (_getindex_one_3d(args[1], i, j, k), _getindex_one_3d(args[2], i, j, k), _getindex_one_3d(args[3], i, j, k))
    else
        return (_getindex_one_3d(args[1], i, j, k), _getindex_one_3d(args[2], i, j, k), _getindex_one_3d(args[3], i, j, k), _getindex_one_3d(args[4], i, j, k))
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
    # Get a representative element from each arg
    sample_args = _get_sample_elements(args)
    # Apply the function to sample values and check the result type
    result = _broadcast_apply(f, sample_args)
    if isa(result, Int64)
        return Int64
    elseif isa(result, Float64)
        return Float64
    elseif isa(result, Bool)
        return Bool
    elseif isa(result, Complex)
        # Complex results need proper complex-typed arrays (Issue #2688)
        return Complex{Float64}
    else
        return Any
    end
end

# Helper: get a sample element from each argument
function _get_sample_elements(args)
    n = length(args)
    if n == 0
        return ()
    elseif n == 1
        return (_get_first_element(args[1]),)
    elseif n == 2
        return (_get_first_element(args[1]), _get_first_element(args[2]))
    elseif n == 3
        return (_get_first_element(args[1]), _get_first_element(args[2]), _get_first_element(args[3]))
    else
        return (_get_first_element(args[1]), _get_first_element(args[2]))
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
function similar(bc::Broadcasted, ElType::Type)
    ax = axes(bc)
    nd = length(ax)
    # Calculate total element count
    if nd == 0
        n = 1
    elseif nd == 1
        n = length(ax[1])
    elseif nd == 2
        n = length(ax[1]) * length(ax[2])
    else
        n = 1
        for i in 1:nd
            n = n * length(ax[i])
        end
    end
    # Create typed array using compile-time type literals
    tname = string(ElType)
    if tname == "Float64"
        arr = Vector{Float64}(undef, n)
    elseif tname == "Int64"
        arr = Vector{Int64}(undef, n)
    elseif tname == "Bool"
        arr = Vector{Bool}(undef, n)
    elseif length(tname) >= 7 && tname[1:7] == "Complex"
        arr = Vector{Complex{Float64}}(undef, n)
    else
        arr = Vector{Any}(undef, n)
    end
    # Reshape for 2D
    if nd == 2
        return reshape(arr, length(ax[1]), length(ax[2]))
    end
    return arr
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
function broadcast_unalias(dest, src)
    if dest === src
        # Same object: make a copy to avoid aliasing
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
        return (preprocess(dest, args[1]),)
    elseif n == 2
        return (preprocess(dest, args[1]), preprocess(dest, args[2]))
    elseif n == 3
        return (preprocess(dest, args[1]), preprocess(dest, args[2]), preprocess(dest, args[3]))
    elseif n == 4
        return (preprocess(dest, args[1]), preprocess(dest, args[2]), preprocess(dest, args[3]), preprocess(dest, args[4]))
    else
        return (preprocess(dest, args[1]), preprocess(dest, args[2]))
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
function copy(bc::Broadcasted)
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
    for i in 1:n
        dest[i] = _broadcast_apply(bc.f, (a[i], b[i]))
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

    if scalar_left
        for i in 1:n
            dest[i] = bc.f(scalar, arr[i])
        end
    else
        for i in 1:n
            dest[i] = bc.f(arr[i], scalar)
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

    # Preserve aliasing semantics for direct and nested Broadcasted args.
    if _fastpath_arg_refs_dest_array(a, dest) || _fastpath_arg_refs_dest_array(b, dest)
        return false
    end

    rows = s[1]
    cols = s[2]
    if !_fastpath_2d_arg_compatible(a, rows, cols) || !_fastpath_2d_arg_compatible(b, rows, cols)
        return false
    end

    for j in 1:cols
        for i in 1:rows
            linear = i + (j - 1) * rows
            dest[linear] = bc.f(_getindex_one_2d(a, i, j), _getindex_one_2d(b, i, j))
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
            dest[i] = _broadcast_getindex(bc_preprocessed, i)
        end
    else
        # Multi-dimensional: convert linear index to CartesianIndex (Issue #2689)
        # This ensures Extruded arrays are indexed correctly per-dimension,
        # allowing proper broadcast dimension mapping (e.g., [3] .+ zeros(3,2)).
        for i in 1:n
            ci = _linear_to_cartesian(i, s)
            dest[i] = _broadcast_getindex(bc_preprocessed, ci)
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
function materialize(x)
    return x
end

# materialize!: in-place materialization
function materialize!(dest, bc::Broadcasted)
    ibc = instantiate(Broadcasted(bc.style, bc.f, bc.bc_args, axes(dest)))
    return copyto!(dest, ibc)
end

# materialize! for non-Broadcasted source: treat as identity broadcast
function materialize!(dest, x)
    n = length(dest)
    for i in 1:n
        if isa(x, Array)
            dest[i] = x[i]
        else
            dest[i] = x
        end
    end
    return dest
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
    return materialize(broadcasted(f, A))
end
function broadcast(f, A, B)
    return materialize(broadcasted(f, A, B))
end
function broadcast(f, A, B, C)
    return materialize(broadcasted(f, A, B, C))
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
    materialize!(dest, broadcasted(f, A))
    return dest
end
function broadcast!(f, dest, A, B)
    materialize!(dest, broadcasted(f, A, B))
    return dest
end
function broadcast!(f, dest, A, B, C)
    materialize!(dest, broadcasted(f, A, B, C))
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
