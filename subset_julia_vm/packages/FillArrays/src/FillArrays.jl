module FillArrays

# Minimal FillArrays compatibility stub for the Optim.jl MVP (Issue #7478).
#
# Upstream FillArrays provides lazy constant arrays (`Zeros`, `Ones`, `Fill`)
# used by some Optim preconditioners and bound representations.  The
# SubsetJuliaVM Optim MVP allocates dense `Vector{Float64}` instead, so these
# helpers simply materialize ordinary dense arrays.  Lazy/structured behavior
# is deferred (see docs/vm/OPTIM.md).

export Zeros, Ones, Fill

Zeros(n::Integer) = fill(0.0, n)
Ones(n::Integer) = fill(1.0, n)
Fill(value, n::Integer) = fill(value, n)

end # module FillArrays
