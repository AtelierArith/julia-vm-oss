module PositiveFactorizations

# Minimal PositiveFactorizations compatibility stub for the Optim.jl MVP
# (Issue #7478).
#
# Upstream PositiveFactorizations provides positive-definite Cholesky
# factorizations used by Optim's Newton/quasi-Newton solvers to guarantee a
# descent direction from an indefinite Hessian.  None of the MVP solvers
# (GoldenSection, Brent, NelderMead, GradientDescent) require a Hessian, so this
# stub only exposes the `Positive` marker so that `using PositiveFactorizations`
# resolves.  Second-order solvers are deferred (see docs/vm/OPTIM.md).

export Positive

struct Positive end

end # module PositiveFactorizations
