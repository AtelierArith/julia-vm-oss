# maximize wrappers. Adapted from upstream `src/maximize.jl`.
#
# `maximize` minimizes the negated objective and wraps the result so that the
# public query API reports maximizer/maximum with the expected sign.

struct MaximizationWrapper
    res
end
_res(r::MaximizationWrapper) = r.res

# ── Univariate ───────────────────────────────────────────────────────────────
function maximize(f, lb::Real, ub::Real, method::UnivariateOptimizer; kwargs...)
    fmax = x -> -f(x)
    return MaximizationWrapper(optimize(fmax, lb, ub, method; kwargs...))
end

function maximize(f, lb::Real, ub::Real; kwargs...)
    fmax = x -> -f(x)
    return MaximizationWrapper(optimize(fmax, lb, ub; kwargs...))
end

# ── Multivariate (derivative-free) ───────────────────────────────────────────
function maximize(f, x0::AbstractArray, method::AbstractOptimizer, options::Options = Options())
    fmax = x -> -f(x)
    return MaximizationWrapper(optimize(fmax, x0, method, options))
end

function maximize(f, x0::AbstractArray, options::Options = Options())
    fmax = x -> -f(x)
    return MaximizationWrapper(optimize(fmax, x0, NelderMead(), options))
end

# ── Query API on the maximization wrapper ────────────────────────────────────
maximizer(r::MaximizationWrapper) = minimizer(_res(r))
maximum(r::MaximizationWrapper) = -minimum(_res(r))
minimizer(r::MaximizationWrapper) = minimizer(_res(r))
iterations(r::MaximizationWrapper) = iterations(_res(r))
converged(r::MaximizationWrapper) = converged(_res(r))
f_calls(r::MaximizationWrapper) = f_calls(_res(r))
lower_bound(r::MaximizationWrapper) = lower_bound(_res(r))
upper_bound(r::MaximizationWrapper) = upper_bound(_res(r))
summary(io::IO, r::MaximizationWrapper) = summary(io, _res(r))

function show(io::IO, r::MaximizationWrapper)
    println(io, "Results of Maximization Algorithm")
    print(io, " * Algorithm: ")
    summary(io, _res(r))
    println(io)
    println(io, " * Maximizer: ", maximizer(r))
    print(io, " * Maximum: ", maximum(r))
    return
end
