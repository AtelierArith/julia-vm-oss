# First-order gradient descent with a user-supplied gradient. Adapted from
# upstream `src/multivariate/solvers/first_order/gradient_descent.jl`.
#
# The search direction is the negative gradient; the step is chosen by an Armijo
# (sufficient-decrease) backtracking line search.  Upstream defaults to the
# HagerZhang line search; for the MVP we use BackTracking, which converges to the
# same minimizer/minimum within tolerance for the supported convex problems.
# Full HagerZhang / quasi-Newton line searches are deferred (see docs/vm/OPTIM.md).

"""
    GradientDescent(; linesearch = LineSearches.BackTracking(), alphaguess = 1.0)

Steepest-descent optimizer for use with a user-provided in-place gradient
`g!(G, x)`.
"""
struct GradientDescent <: FirstOrderOptimizer
    linesearch
    alpha0::Float64
end
GradientDescent(; linesearch = BackTracking(), alphaguess = 1.0, P = nothing) =
    GradientDescent(linesearch, Float64(alphaguess))

summary(io::IO, ::GradientDescent) = print(io, "Gradient Descent")

# Armijo backtracking line search along direction `s` from `x` with value `fx`.
# Returns the accepted step length `alpha`.
function _backtracking_step(d, x, fx, s, dphi0, alpha0, c_1, rho, maxls)
    alpha = alpha0
    x_new = _axpy(x, alpha, s)
    f_new = value(d, x_new)
    ls = 0
    while f_new > fx + c_1 * alpha * dphi0 && ls < maxls
        alpha = alpha * rho
        x_new = _axpy(x, alpha, s)
        f_new = value(d, x_new)
        ls += 1
    end
    return alpha
end

function _gradient_descent(d, x0, method, options)
    x = Float64[Float64(xi) for xi in x0]
    initial_x = copy(x)

    vg = value_gradient!(d, x)
    fx = vg[1]
    g = vg[2]

    # Line search tuning (from BackTracking, else sensible defaults).
    c_1 = 1.0e-4
    rho = 0.5
    maxls = 50
    ls = method.linesearch
    if ls isa BackTracking
        c_1 = ls.c_1
        rho = ls.rho_hi
        maxls = ls.iterations
    end

    iteration = 0
    gnorm = _maxabs(g)
    x_abschange = 0.0
    f_abschange = 0.0
    is_g_converged = gnorm <= options.g_abstol
    is_x_converged = false
    is_f_converged = false

    while !(is_g_converged || is_x_converged || is_f_converged) &&
              iteration < options.iterations
        iteration += 1

        s = Float64[-g[i] for i in eachindex(g)]  # s = -g
        dphi0 = _dot(g, s)

        alpha = _backtracking_step(d, x, fx, s, dphi0, method.alpha0, c_1, rho, maxls)

        x_prev = x
        f_prev = fx
        x = _axpy(x, alpha, s)
        vg = value_gradient!(d, x)
        fx = vg[1]
        g = vg[2]

        gnorm = _maxabs(g)
        x_abschange = _maxabs(_sub(x, x_prev))
        f_abschange = abs(fx - f_prev)

        is_g_converged = gnorm <= options.g_abstol
        is_x_converged = options.x_abstol > 0.0 && x_abschange <= options.x_abstol
        is_f_converged = options.f_abstol > 0.0 && f_abschange <= options.f_abstol
    end

    return MultivariateOptimizationResults(
        method,
        initial_x,
        x,
        fx,
        iteration,
        options.x_abstol,
        options.x_reltol,
        x_abschange,
        options.f_abstol,
        options.f_reltol,
        f_abschange,
        options.g_abstol,
        gnorm,
        f_calls(d),
        g_calls(d),
        is_x_converged,
        is_f_converged,
        is_g_converged,
        options.time_limit,
        0.0,
    )
end
