# Univariate result type. Adapted from upstream `src/univariate/types.jl`.

"""
    UnivariateOptimizationResults

Result of a bounded univariate optimization run (GoldenSection, Brent).
"""
mutable struct UnivariateOptimizationResults <: OptimizationResults
    method
    initial_lower::Float64
    initial_upper::Float64
    minimizer::Float64
    minimum::Float64
    iterations::Int
    rel_tol::Float64
    abs_tol::Float64
    f_calls::Int
    converged::Bool
end
