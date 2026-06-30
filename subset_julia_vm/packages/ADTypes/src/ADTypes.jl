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

struct AutoForwardDiff <: AbstractADType end
AutoForwardDiff(; kwargs...) = AutoForwardDiff()

struct AutoFiniteDiff <: AbstractADType end
AutoFiniteDiff(; kwargs...) = AutoFiniteDiff()

struct AutoReverseDiff <: AbstractADType end
AutoReverseDiff(; kwargs...) = AutoReverseDiff()

struct AutoZygote <: AbstractADType end
AutoZygote(; kwargs...) = AutoZygote()

end # module ADTypes
