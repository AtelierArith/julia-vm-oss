module ADTypes

# Minimal ADTypes compatibility stub for the Optim.jl MVP (Issue #7478).
#
# Upstream ADTypes is the package that defines the automatic-differentiation
# backend marker types (`AutoForwardDiff`, `AutoFiniteDiff`, ...) that Optim
# uses to select how gradients/Hessians are produced.  The SubsetJuliaVM Optim
# MVP only supports user-supplied gradients, so no AD backend is wired up.
# These marker types exist purely so that `using ADTypes` resolves and code
# that names a backend can load.  Selecting any of them for an actual
# differentiation request is deferred (see docs/vm/OPTIM.md).

export AbstractADType, AutoForwardDiff, AutoFiniteDiff, AutoReverseDiff, AutoZygote

abstract type AbstractADType end

struct AutoForwardDiff <: AbstractADType
    options
end
AutoForwardDiff(; kwargs...) = AutoForwardDiff(kwargs)

struct AutoFiniteDiff <: AbstractADType
    options
end
AutoFiniteDiff(; kwargs...) = AutoFiniteDiff(kwargs)

struct AutoReverseDiff <: AbstractADType
    options
end
AutoReverseDiff(; kwargs...) = AutoReverseDiff(kwargs)

struct AutoZygote <: AbstractADType
    options
end
AutoZygote(; kwargs...) = AutoZygote(kwargs)

end # module ADTypes
