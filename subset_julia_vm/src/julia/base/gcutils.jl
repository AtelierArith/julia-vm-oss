# =============================================================================
# GC utilities, WeakRef, and finalizers
# =============================================================================
# Based on Julia's base/gcutils.jl. The VM owns weak-reference clearing and
# finalizer scheduling; this file provides the public Julia surface.

mutable struct WeakRef
    value::Any

    WeakRef() = new(nothing)
end

WeakRef(x) = _weakref_new(x)

function getproperty(w::WeakRef, s::Symbol)
    if s === :value
        return _weakref_value(w)
    end
    return getfield(w, s)
end

function setproperty!(w::WeakRef, s::Symbol, v)
    if s === :value
        return _weakref_set_value!(w, v)
    end
    return setfield!(w, s, v)
end

finalizer(f, x) = _finalizer(f, x)
finalize(x) = _finalize(x)

module GC
    gc(full::Bool=true) = Base._gc_collect(full)
    safepoint() = Base._gc_safepoint()
    in_finalizer() = Base._gc_in_finalizer()
end
