# Static array broadcast support (Issue #7460 Phase 4; Issue #8161 mixed/nested).
#
# sjulia lowers `v .+ 10`, `sin.(v)`, `v .+ w`, `abs.(v .- w)`, ... to
# `materialize(Broadcasted(f, (args...)))`, fusing a `.`-call chain into a *tree*
# of nested `Broadcasted` nodes. The generic Base pipeline only recognises
# `Array`/`Tuple`/range operands as broadcast containers, so a static-array
# operand is treated as a 0-dimensional scalar: `v .+ 10` collapses to the
# invalid `+(v, 10)`, and `v .- w` (w::Vector) collapses to `-(v, w[i])`.
#
# The `Base._STATIC_BROADCAST_HOOK` Ref (base/broadcast.jl) is installed at the
# bottom of this file with `_static_broadcast_impl`. It reproduces upstream
# StaticArrays' `BroadcastStyle` precedence
# (`StaticArrayStyle ⊙ DefaultArrayStyle{0}` → `StaticArrayStyle`;
#  `StaticArrayStyle ⊙ DefaultArrayStyle{N≥1}` → `DefaultArrayStyle`) by
# classifying the whole operand tree, recursing into fused nested `Broadcasted`:
#
#   * static array(s) mixed only with scalars      → STATIC result (SVector/SMatrix)
#   * static array mixed with any *dynamic* array  → DYNAMIC result (plain Array)
#
# For the dynamic case upstream returns a `Sized*` array (a dynamic Array with a
# statically known size); the subset has no `Sized*` types, so a plain `Array`
# reproduces the upstream values, element type, and display. Returning `nothing`
# (no static operand anywhere in the tree) defers to the generic pipeline.

# --- Operand-tree classification (recurses into fused nested `Broadcasted`) ---
# The tree is walked via `Base._is_broadcasted` / `Base._broadcasted_args` so the
# Base `Broadcasted` type is never named cross-module and no fragile struct-typed
# dispatch is introduced.

# A StaticArray leaf anywhere in the tree?
function _static_bcast_has_static(x)
    if x isa StaticArray
        return true
    elseif Base._is_broadcasted(x)
        for a in Base._broadcasted_args(x)
            _static_bcast_has_static(a) && return true
        end
    end
    return false
end

# A dynamic (non-static) broadcast *container* leaf anywhere in the tree?
# (Array/SubArray/Tuple/range — anything Base assigns a non-scalar shape.)
function _static_bcast_has_dynamic(x)
    if x isa StaticArray
        return false
    elseif Base._is_broadcasted(x)
        for a in Base._broadcasted_args(x)
            _static_bcast_has_dynamic(a) && return true
        end
        return false
    end
    return Base._broadcastable_shape(x) != ()
end

# First StaticArray leaf in the tree (fixes the static result shape/length).
function _static_bcast_ref(x)
    if x isa StaticArray
        return x
    elseif Base._is_broadcasted(x)
        for a in Base._broadcasted_args(x)
            r = _static_bcast_ref(a)
            r === nothing || return r
        end
    end
    return nothing
end

# Replace every StaticArray leaf with a plain `Array`, preserving the
# `Broadcasted` tree so the generic dynamic pipeline can materialize it.
function _static_bcast_destatic(x)
    if x isa StaticArray
        return collect(x)
    elseif Base._is_broadcasted(x)
        return Base._make_broadcasted(Base._broadcasted_f(x),
                                      map(_static_bcast_destatic, Base._broadcasted_args(x)))
    end
    return x
end

# Every static leaf must share the reference length (element-wise broadcast).
function _static_bcast_check_lengths(x, n::Int64)
    if x isa StaticArray
        if length(x) != n
            throw(DimensionMismatch("static broadcast: operand length $(length(x)) does not match $(n)"))
        end
    elseif Base._is_broadcasted(x)
        for a in Base._broadcasted_args(x)
            _static_bcast_check_lengths(a, n)
        end
    end
    return nothing
end

# Per-operand element accessor: index static-array leaves, evaluate nested
# `Broadcasted` recursively, pass scalars through. Only reached on the STATIC
# path, where every non-static leaf is a true scalar.
function _static_bcast_elem(a, i::Int64)
    if a isa StaticArray
        return a[i]
    elseif Base._is_broadcasted(a)
        return Base._broadcasted_f(a)(map(x -> _static_bcast_elem(x, i),
                                          Base._broadcasted_args(a))...)
    end
    return a
end

function _static_broadcast_impl(f, args)
    # Not our broadcast unless a static array participates somewhere in the tree.
    have_static = false
    for a in args
        if _static_bcast_has_static(a)
            have_static = true
            break
        end
    end
    have_static || return nothing

    # Mixed static/dynamic → DYNAMIC result (Issue #8161). Replace every static
    # leaf with a plain `Array` and re-materialize through the generic pipeline,
    # which combines the static + dynamic shapes and yields a properly typed
    # dynamic `Array`. The rebuilt tree has no static leaf, so the hook declines
    # the re-entry and the generic pipeline runs.
    for a in args
        if _static_bcast_has_dynamic(a)
            destatic = map(_static_bcast_destatic, args)
            return Base._materialize_broadcasted(f, destatic)
        end
    end

    # Static-only (static arrays + scalars) → STATIC result (SVector/SMatrix).
    sref = nothing
    for a in args
        sref = _static_bcast_ref(a)
        sref === nothing || break
    end
    n = length(sref)
    for a in args
        _static_bcast_check_lengths(a, n)
    end
    vals = _static_bcast_values(f, args, n)
    return _static_bcast_build(sref, vals)
end

# Apply `f` element-wise, returning a length-`n` Vector of results.
function _static_bcast_values(f, args, n::Int64)
    out = []
    for i in 1:n
        elems = []
        for a in args
            push!(elems, _static_bcast_elem(a, i))
        end
        push!(out, f(elems...))
    end
    return out
end

# Rebuild a static result of the same shape as the reference operand.
_static_bcast_build(sref::StaticVector, vals) = SVector(vals...)

function _static_bcast_build(sref::StaticMatrix, vals)
    s = size(sref)
    m = s[1]
    n = s[2]
    # `vals` is in column-major linear order (matching the static matrix layout),
    # which is exactly what the flat-tuple SMatrix constructor expects (Issue
    # #8084). Runtime-parameter SMatrix{M,N} construction is unsupported (Issue
    # #8125), so the inline square sizes are handled with literal constructors.
    if m == 2 && n == 2
        return SMatrix{2,2}((vals[1], vals[2], vals[3], vals[4]))
    elseif m == 3 && n == 3
        return SMatrix{3,3}((vals[1], vals[2], vals[3],
                             vals[4], vals[5], vals[6],
                             vals[7], vals[8], vals[9]))
    elseif m == 4 && n == 4
        return SMatrix{4,4}((vals[1], vals[2], vals[3], vals[4],
                             vals[5], vals[6], vals[7], vals[8],
                             vals[9], vals[10], vals[11], vals[12],
                             vals[13], vals[14], vals[15], vals[16]))
    end
    # Non-inline matrix shapes are out of scope for the Phase 4 MVP.
    return nothing
end

# Install the static-broadcast callback so `copy(::Broadcasted)` (base) routes
# static-operand broadcasts here. Runs once at package load (Issue #7460).
Base._set_static_broadcast_hook!(_static_broadcast_impl)
