module EnumX

# Minimal EnumX compatibility stub for the Optim.jl MVP (Issue #7478).
#
# Upstream EnumX provides the `@enumx` macro, which defines a namespaced enum
# (a baremodule holding the variants).  Upstream Optim uses it for its
# `TerminationCode` enum.  The SubsetJuliaVM Optim MVP models termination via
# plain boolean convergence flags on the result structs instead of a namespaced
# enum, so `@enumx` is never invoked here.  This stub exists only so that
# `using EnumX` resolves; full `@enumx` semantics are deferred.

export @enumx

macro enumx(args...)
    # No-op stub: @enumx is not used by the Optim MVP. Defining an actual
    # namespaced enum is deferred (see docs/vm/OPTIM.md).
    return nothing
end

end # module EnumX
