# Public query API on optimization results.
# Adapted from upstream `src/api.jl` for the SubsetJuliaVM MVP.

summary(io::IO, r::OptimizationResults) = summary(io, r.method)

# ── Shared accessors ─────────────────────────────────────────────────────────
minimizer(r::OptimizationResults) = r.minimizer
minimum(r::OptimizationResults) = r.minimum
iterations(r::OptimizationResults) = r.iterations
f_calls(r::OptimizationResults) = r.f_calls

# ── Univariate ───────────────────────────────────────────────────────────────
converged(r::UnivariateOptimizationResults) = r.converged
lower_bound(r::UnivariateOptimizationResults) = r.initial_lower
upper_bound(r::UnivariateOptimizationResults) = r.initial_upper
rel_tol(r::UnivariateOptimizationResults) = r.rel_tol
abs_tol(r::UnivariateOptimizationResults) = r.abs_tol

# ── Multivariate ─────────────────────────────────────────────────────────────
function converged(r::MultivariateOptimizationResults)
    return r.x_converged || r.f_converged || r.g_converged
end
g_calls(r::MultivariateOptimizationResults) = r.g_calls
x_converged(r::MultivariateOptimizationResults) = r.x_converged
f_converged(r::MultivariateOptimizationResults) = r.f_converged
g_converged(r::MultivariateOptimizationResults) = r.g_converged
initial_x(r::MultivariateOptimizationResults) = r.initial_x
g_residual(r::MultivariateOptimizationResults) = r.g_residual
x_abschange(r::MultivariateOptimizationResults) = r.x_abschange
f_abschange(r::MultivariateOptimizationResults) = r.f_abschange

# ── Minimal display (intentionally narrow; not byte-compatible with upstream) ─
function show(io::IO, r::UnivariateOptimizationResults)
    println(io, "Results of Optimization Algorithm")
    print(io, " * Algorithm: ")
    summary(io, r.method)
    println(io)
    println(io, " * Search Interval: [", r.initial_lower, ", ", r.initial_upper, "]")
    println(io, " * Minimizer: ", r.minimizer)
    println(io, " * Minimum: ", r.minimum)
    println(io, " * Iterations: ", r.iterations)
    println(io, " * Convergence: ", r.converged)
    print(io, " * Objective Function Calls: ", r.f_calls)
    return
end

function show(io::IO, r::MultivariateOptimizationResults)
    println(io, "Results of Optimization Algorithm")
    print(io, " * Algorithm: ")
    summary(io, r.method)
    println(io)
    println(io, " * Minimizer: ", r.minimizer)
    println(io, " * Minimum: ", r.minimum)
    println(io, " * Iterations: ", r.iterations)
    println(io, " * Converged: ", converged(r))
    println(io, " * f(x) calls: ", r.f_calls)
    print(io, " * ∇f(x) calls: ", r.g_calls)
    return
end
