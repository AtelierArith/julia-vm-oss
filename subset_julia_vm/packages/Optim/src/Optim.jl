module Optim

# SubsetJuliaVM Optim.jl MVP (Issue #7432).
#
# This is an upstream-adapted, pure-Julia MVP of Optim.jl targeting the no-JIT
# iOS runtime.  It implements the deterministic, no-AD / user-gradient solver
# workflows: bounded univariate minimization (GoldenSection, Brent),
# derivative-free multivariate minimization (NelderMead), and first-order
# user-gradient minimization (GradientDescent) with an Armijo backtracking line
# search.  The public result/query API (minimizer, minimum, iterations,
# converged, f_calls, g_calls, maximize, ...) mirrors upstream Optim.
#
# Advanced solvers (BFGS/LBFGS/CG, Newton/trust-region, constrained Fminbox/
# IPNewton/SAMIN), automatic differentiation, full LineSearches, and trace
# printing are intentionally deferred.  See docs/vm/OPTIM.md for the supported /
# deferred surface map and the dependency-stub policy.
#
# The upstream-like directory layout under src/ is preserved so the MVP can be
# expanded toward upstream parity without restructuring.

import Base: minimum, maximum, summary, show

using NLSolversBase
import NLSolversBase: f_calls, g_calls

using LineSearches

export optimize,
    maximize,
    # Solvers
    GoldenSection,
    Brent,
    NelderMead,
    GradientDescent,
    BFGS,
    AdaptiveParameters,
    FixedParameters,
    AffineSimplexer,
    # Configuration
    Options,
    # Objective re-exports from NLSolversBase
    NonDifferentiable,
    OnceDifferentiable,
    # Result types
    OptimizationResults,
    UnivariateOptimizationResults,
    MultivariateOptimizationResults,
    # Query API
    minimizer,
    maximizer,
    iterations,
    converged,
    x_converged,
    f_converged,
    g_converged,
    f_calls,
    g_calls,
    initial_x,
    lower_bound,
    upper_bound,
    rel_tol,
    abs_tol,
    g_residual

include("types.jl")
include("univariate/types.jl")
include("api.jl")
include("utilities/generic.jl")
include("univariate/solvers/golden_section.jl")
include("univariate/solvers/brent.jl")
include("univariate/optimize/interface.jl")
include("multivariate/solvers/zeroth_order/nelder_mead.jl")
include("multivariate/solvers/first_order/gradient_descent.jl")
include("multivariate/solvers/first_order/bfgs.jl")
include("multivariate/optimize/interface.jl")
include("maximize.jl")

end # module Optim
