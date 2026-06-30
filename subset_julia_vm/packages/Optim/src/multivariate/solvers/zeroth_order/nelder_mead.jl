# Nelder-Mead derivative-free simplex method. Adapted from upstream
# `src/multivariate/solvers/zeroth_order/nelder_mead.jl`.
#
# The accept/reject logic (reflect → expand / accept / outside-contract /
# inside-contract / shrink) and the adaptive parameters mirror upstream.  The
# simplex is re-sorted each iteration instead of maintaining the `i_order`
# rotation, which yields the same trajectory while keeping the loop simple.

struct AffineSimplexer
    a::Float64
    b::Float64
end
AffineSimplexer(; a = 0.025, b = 0.5) = AffineSimplexer(a, b)

struct AdaptiveParameters
    alpha::Float64
    beta::Float64
    gamma::Float64
    delta::Float64
end
AdaptiveParameters(; alpha = 1.0, beta = 1.0, gamma = 0.75, delta = 1.0) =
    AdaptiveParameters(alpha, beta, gamma, delta)

struct FixedParameters
    alpha::Float64
    beta::Float64
    gamma::Float64
    delta::Float64
end
FixedParameters(; alpha = 1.0, beta = 2.0, gamma = 0.5, delta = 0.5) =
    FixedParameters(alpha, beta, gamma, delta)

_nm_parameters(p::AdaptiveParameters, n) =
    (p.alpha, p.beta + 2.0 / n, p.gamma - 1.0 / (2 * n), p.delta - 1.0 / n)
_nm_parameters(p::FixedParameters, n) = (p.alpha, p.beta, p.gamma, p.delta)

"""
    NelderMead(; parameters = AdaptiveParameters(), initial_simplex = AffineSimplexer())

Derivative-free Nelder-Mead simplex optimizer.
"""
struct NelderMead <: ZerothOrderOptimizer
    initial_simplex
    parameters
end
NelderMead(; initial_simplex = AffineSimplexer(), parameters = AdaptiveParameters()) =
    NelderMead(initial_simplex, parameters)

summary(io::IO, ::NelderMead) = print(io, "Nelder-Mead")

# Nelder-Mead simplex stopping objective: √(var(f) · n/m).
_nmobjective(y, nx, mverts) = sqrt(_var(y) * (nx / mverts))

function _simplexer(s::AffineSimplexer, x0)
    n = length(x0)
    simplex = [copy(x0)]
    for j in 1:n
        v = copy(x0)
        v[j] = (1.0 + s.b) * v[j] + s.a
        push!(simplex, v)
    end
    return simplex
end

function _sub(a, b)
    return Float64[a[i] - b[i] for i in eachindex(a)]
end

# a + c * v  (elementwise)
function _axpy(a, c, v)
    return Float64[a[i] + c * v[i] for i in eachindex(a)]
end

# Centroid of all vertices except the worst (order[m]).
function _nm_centroid(simplex, order, n)
    dim = length(simplex[1])
    c = fill(0.0, dim)
    for k in 1:n
        v = simplex[order[k]]
        for i in 1:dim
            c[i] = c[i] + v[i]
        end
    end
    for i in 1:dim
        c[i] = c[i] / n
    end
    return c
end

function _nm_shrink!(d, simplex, fvals, ilow, m, delta)
    xlow = simplex[ilow]
    dim = length(xlow)
    for i in 1:m
        if i != ilow
            v = simplex[i]
            nv = Float64[xlow[j] + delta * (v[j] - xlow[j]) for j in 1:dim]
            simplex[i] = nv
            fvals[i] = value(d, nv)
        end
    end
    return nothing
end

function _nm_step!(d, simplex, fvals, n, m, alpha, beta, gamma, delta)
    order = _sortperm(fvals)
    ilow = order[1]
    ihigh = order[m]
    isecond = order[n]
    flow = fvals[ilow]
    fhigh = fvals[ihigh]
    fsecond = fvals[isecond]

    centroid = _nm_centroid(simplex, order, n)
    xhigh = simplex[ihigh]

    # Reflection
    xr = _axpy(centroid, alpha, _sub(centroid, xhigh))
    fr = value(d, xr)

    if fr < flow
        # Expansion
        xe = _axpy(centroid, beta, _sub(xr, centroid))
        fe = value(d, xe)
        if fe < fr
            simplex[ihigh] = xe
            fvals[ihigh] = fe
        else
            simplex[ihigh] = xr
            fvals[ihigh] = fr
        end
    elseif fr < fsecond
        # Accept reflection
        simplex[ihigh] = xr
        fvals[ihigh] = fr
    else
        if fr < fhigh
            # Outside contraction
            xc = _axpy(centroid, gamma, _sub(xr, centroid))
            fc = value(d, xc)
            if fc < fr
                simplex[ihigh] = xc
                fvals[ihigh] = fc
            else
                _nm_shrink!(d, simplex, fvals, ilow, m, delta)
            end
        else
            # Inside contraction
            xc = _axpy(centroid, -gamma, _sub(xr, centroid))
            fc = value(d, xc)
            if fc < fhigh
                simplex[ihigh] = xc
                fvals[ihigh] = fc
            else
                _nm_shrink!(d, simplex, fvals, ilow, m, delta)
            end
        end
    end
    return nothing
end

function _nm_after(d, simplex, fvals, n, m)
    order = _sortperm(fvals)
    xc = _nm_centroid(simplex, order, n)
    fc = value(d, xc)
    (fmin, imin) = _findmin(fvals)
    xmin = simplex[imin]
    if fc < fmin
        xmin = xc
        fmin = fc
    end
    return (copy(xmin), fmin)
end

function _nelder_mead(d, x0, method, options)
    n = length(x0)
    m = n + 1
    x0f = Float64[Float64(xi) for xi in x0]
    simplex = _simplexer(method.initial_simplex, x0f)
    fvals = Float64[value(d, simplex[i]) for i in 1:m]
    params = _nm_parameters(method.parameters, n)
    alpha = params[1]
    beta = params[2]
    gamma = params[3]
    delta = params[4]

    iteration = 0
    nm_x = _nmobjective(fvals, n, m)
    is_g_converged = nm_x <= options.g_abstol

    while !is_g_converged && iteration < options.iterations
        iteration += 1
        _nm_step!(d, simplex, fvals, n, m, alpha, beta, gamma, delta)
        nm_x = _nmobjective(fvals, n, m)
        is_g_converged = nm_x <= options.g_abstol
    end

    after = _nm_after(d, simplex, fvals, n, m)
    xmin = after[1]
    fmin = after[2]

    return MultivariateOptimizationResults(
        method,
        x0f,
        xmin,
        fmin,
        iteration,
        options.x_abstol,
        options.x_reltol,
        0.0,
        options.f_abstol,
        options.f_reltol,
        0.0,
        options.g_abstol,
        nm_x,
        f_calls(d),
        0,
        false,
        false,
        is_g_converged,
        options.time_limit,
        0.0,
    )
end

function optimize(f, x0::AbstractArray, method::NelderMead; kwargs...)
    return optimize(f, x0, method, Options(; kwargs...))
end

function optimize(f, x0::AbstractArray, method::NelderMead, options::Options)
    d = NonDifferentiable(f, x0)
    return _nelder_mead(d, x0, method, options)
end
