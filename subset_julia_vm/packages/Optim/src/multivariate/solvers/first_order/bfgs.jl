# BFGS quasi-Newton optimizer (Issue #8059). Adapted from upstream
# `src/multivariate/solvers/first_order/bfgs.jl`.
#
# BFGS maintains an approximation `invH` to the inverse Hessian, takes the search
# direction `s = -invH * grad`, finds a step with the Hager-Zhang line search, and
# updates `invH` with the Sherman-Morrison BFGS formula (Nocedal & Wright, sec.
# 8.1).  The initial inverse Hessian is the identity (upstream default
# `initial_invH = nothing, initial_stepnorm = nothing`).
#
# This mirrors upstream's defaults exactly: `alphaguess = InitialStatic()`
# (step guess 1.0) and `linesearch = HagerZhang()`.  Both the user-gradient form
# `optimize(f, g!, x0, BFGS())` and the finite-difference form
# `optimize(f, x0, BFGS())` (autodiff = :finite) are supported.
#
# Convergence / call-count parity (see docs/vm/OPTIM.md): for problems that
# converge in one step (well-conditioned quadratics) the minimizer and the
# `f_calls`/`g_calls` counts match upstream exactly.  For multi-step problems
# (e.g. Rosenbrock) the iteration count matches upstream, and the minimizer
# matches to within solver tolerance, but the exact `f_calls`/`g_calls` differ
# because they depend on the floating-point reduction order of `dot`/`mul!`
# (upstream uses BLAS; the no-JIT VM uses scalar loops).

"""
    BFGS(; alphaguess = LineSearches.InitialStatic(),
           linesearch = LineSearches.HagerZhang())

Broyden-Fletcher-Goldfarb-Shanno quasi-Newton optimizer.  Works with a
user-supplied in-place gradient `g!(G, x)` (`optimize(f, g!, x0, BFGS())`) or with
a central finite-difference gradient (`optimize(f, x0, BFGS())`).
"""
struct BFGS <: FirstOrderOptimizer
    alphaguess
    linesearch
end
BFGS(; alphaguess = InitialStatic(), linesearch = HagerZhang(), initial_invH = nothing,
    initial_stepnorm = nothing) = BFGS(alphaguess, linesearch)

summary(io::IO, ::BFGS) = print(io, "BFGS")

# n×n identity inverse-Hessian seed.
function _bfgs_identity(n)
    H = fill(0.0, n, n)
    for i in 1:n
        H[i, i] = 1.0
    end
    return H
end

# In-place s .= -invH * g.
function _bfgs_neg_matvec!(s, invH, g, n)
    for i in 1:n
        acc = 0.0
        for j in 1:n
            acc += invH[i, j] * g[j]
        end
        s[i] = -acc
    end
    return s
end

# The explicit `::MultivariateOptimizationResults` return annotation is a
# load-time performance fix (Issue #8182). Without it, compile-time return-type
# inference of this body explodes (~5 s, 97 % of `using Optim`): the `phidphi`
# closure defined in the loop below is threaded through the deep, mutually
# recursive HagerZhang line-search call tree and re-specialized under the loop
# fixpoint. `_bfgs` always returns a `MultivariateOptimizationResults`, so the
# annotation is exact; it lets `build_method_tables` skip inferring this body
# (5097 ms -> 42 ms).
#
# Do NOT remove this annotation. Issue #8185 established empirically that the
# engine's interprocedural work budget canNOT be tightened to catch this case:
# the un-annotated `_bfgs` blow-up (~174k inference work) is the same order as
# legitimate heavy inference (`using Symbolics` ~159k), so any budget low enough
# to catch `_bfgs` would also widen Symbolics to `Top` and regress it. The budget
# is only a catastrophe (host-OOM-class) backstop; this annotation plus the
# per-package load-time smoke test (`using_optim_load_inference_stays_bounded_8185`)
# are the actual guards. See docs/vm/CHECKLISTS.md.
function _bfgs(d, x0, method, options)::MultivariateOptimizationResults
    x = Float64[Float64(xi) for xi in x0]
    n = length(x)
    initial_x = copy(x)

    # Initial value + gradient (primes the cache).
    value_gradient!(d, x)
    f_x = value(d)

    invH = _bfgs_identity(n)
    x_previous = copy(x)
    g_previous = fill(0.0, n)
    s = fill(0.0, n)
    x_ls = fill(0.0, n)
    dx = fill(0.0, n)
    dg = fill(0.0, n)
    u = fill(0.0, n)

    ls = method.linesearch
    alpha0 = method.alphaguess.alpha   # InitialStatic step guess (default 1.0)

    g = gradient(d)
    gnorm = _maxabs(g)
    x_abschange = 0.0
    f_abschange = 0.0
    is_g_converged = gnorm <= options.g_abstol
    is_x_converged = false
    is_f_converged = false
    counter_f_tol = 0
    converged = is_g_converged

    iteration = 0
    while !converged && iteration < options.iterations
        iteration += 1

        g = gradient(d)
        # Search direction s = -invH * g.
        _bfgs_neg_matvec!(s, invH, g, n)

        for i in 1:n
            g_previous[i] = g[i]
        end

        # Line search.
        dphi_0 = _dot(g, s)
        if dphi_0 >= 0.0
            # Reset to identity / steepest descent if the direction is corrupt.
            for i in 1:n
                for j in 1:n
                    invH[i, j] = (i == j) ? 1.0 : 0.0
                end
                s[i] = -g[i]
            end
            dphi_0 = _dot(g, s)
        end
        phi_0 = value(d)

        # InitialStatic alpha guess.
        alpha = alpha0
        x_prev_f = phi_0
        for i in 1:n
            x_previous[i] = x[i]
        end

        # ϕ(α), ϕ'(α) along the search direction.
        phidphi = function (al)
            for i in 1:n
                x_ls[i] = muladd(al, s[i], x[i])
            end
            fv, gv = value_gradient!(d, x_ls)
            return fv, _dot(gv, s)
        end

        ls_success = true
        try
            alpha, _ = hagerzhang_search(ls, phidphi, alpha, phi_0, dphi_0)
        catch ex
            if ex isa LineSearchException
                alpha = ex.alpha
                ls_success = false
            else
                rethrow(ex)
            end
        end

        # x .+= alpha * s
        for i in 1:n
            dx[i] = alpha * s[i]
            x[i] = x[i] + dx[i]
        end
        if !ls_success
            break
        end

        # Refresh value + gradient at the accepted point.
        value_gradient!(d, x)
        f_prev = x_prev_f
        f_x = value(d)
        g = gradient(d)

        gnorm = _maxabs(g)
        x_abschange = _maxabs(_sub(x, x_previous))
        f_abschange = abs(f_x - f_prev)

        is_g_converged = gnorm <= options.g_abstol
        is_x_converged = x_abschange <= options.x_abstol
        is_f_converged = f_abschange <= options.f_abstol
        counter_f_tol = is_f_converged ? counter_f_tol + 1 : 0
        converged = is_x_converged || is_g_converged || (counter_f_tol > 1)
        if converged
            break
        end

        # BFGS inverse-Hessian update (Sherman-Morrison).
        for i in 1:n
            dg[i] = g[i] - g_previous[i]
        end
        dx_dg = _dot(dx, dg)
        if dx_dg > 0.0
            for i in 1:n
                acc = 0.0
                for j in 1:n
                    acc += invH[i, j] * dg[j]
                end
                u[i] = acc
            end
            c1 = (dx_dg + _dot(dg, u)) / (dx_dg * dx_dg)
            c2 = 1.0 / dx_dg
            for j in 1:n
                c1dxj = c1 * dx[j]
                c2dxj = c2 * dx[j]
                c2uj = c2 * u[j]
                for i in 1:n
                    invH[i, j] = muladd(dx[i], c1dxj, muladd(-u[i], c2dxj, muladd(c2uj, -dx[i], invH[i, j])))
                end
            end
        end
    end

    return MultivariateOptimizationResults(
        method,
        initial_x,
        x,
        f_x,
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
