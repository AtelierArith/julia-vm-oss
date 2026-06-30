# Core type hierarchy, configuration options, and the multivariate result type.
# Adapted from upstream Optim `src/types.jl` for the SubsetJuliaVM MVP.

abstract type AbstractOptimizer end
abstract type AbstractConstrainedOptimizer <: AbstractOptimizer end
abstract type ZerothOrderOptimizer <: AbstractOptimizer end
abstract type FirstOrderOptimizer <: AbstractOptimizer end
abstract type SecondOrderOptimizer <: AbstractOptimizer end
abstract type UnivariateOptimizer <: AbstractOptimizer end

abstract type OptimizationResults end

"""
    Options(; kwargs...)

Configurable optimizer options for the Optim MVP.  Unspecified options take the
upstream-compatible defaults (`g_abstol = 1e-8`, `iterations = 1_000`, the other
tolerances `0.0`).  The deprecated `x_tol`/`f_tol`/`g_tol` aliases are accepted
and mapped onto `x_abstol`/`f_reltol`/`g_abstol` respectively, matching upstream.
"""
struct Options
    x_abstol::Float64
    x_reltol::Float64
    f_abstol::Float64
    f_reltol::Float64
    g_abstol::Float64
    iterations::Int
    store_trace::Bool
    show_trace::Bool
    extended_trace::Bool
    show_warnings::Bool
    show_every::Int
    callback
    time_limit::Float64
end

function Options(;
    x_abstol = 0.0,
    x_reltol = 0.0,
    f_abstol = 0.0,
    f_reltol = 0.0,
    g_abstol = 1e-8,
    iterations = 1_000,
    store_trace = false,
    show_trace = false,
    extended_trace = false,
    show_warnings = true,
    show_every = 1,
    callback = nothing,
    time_limit = NaN,
    x_tol = nothing,
    f_tol = nothing,
    g_tol = nothing,
)
    if x_tol !== nothing
        x_abstol = x_tol
    end
    if g_tol !== nothing
        g_abstol = g_tol
    end
    if f_tol !== nothing
        f_reltol = f_tol
    end
    show_every = show_every > 0 ? show_every : 1
    return Options(
        Float64(x_abstol),
        Float64(x_reltol),
        Float64(f_abstol),
        Float64(f_reltol),
        Float64(g_abstol),
        Int(iterations),
        store_trace,
        show_trace,
        extended_trace,
        show_warnings,
        Int(show_every),
        callback,
        Float64(time_limit),
    )
end

"""
    MultivariateOptimizationResults

Result of a multivariate optimization run (NelderMead, GradientDescent).  Carries
the minimizer/minimum, work counters, and the boolean convergence flags consulted
by the public query API.
"""
mutable struct MultivariateOptimizationResults <: OptimizationResults
    method
    initial_x
    minimizer
    minimum::Float64
    iterations::Int
    x_abstol::Float64
    x_reltol::Float64
    x_abschange::Float64
    f_abstol::Float64
    f_reltol::Float64
    f_abschange::Float64
    g_abstol::Float64
    g_residual::Float64
    f_calls::Int
    g_calls::Int
    x_converged::Bool
    f_converged::Bool
    g_converged::Bool
    time_limit::Float64
    time_run::Float64
end
