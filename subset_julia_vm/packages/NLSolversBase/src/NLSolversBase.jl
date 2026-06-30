module NLSolversBase

# Minimal NLSolversBase compatibility surface for the Optim.jl MVP (Issue #7481/#7482).
#
# Upstream NLSolversBase provides a rich `AbstractObjective` hierarchy that caches
# the most recently computed value/gradient/Hessian and supports automatic
# differentiation backends.  The Optim MVP in SubsetJuliaVM provides:
#   * `NonDifferentiable` for derivative-free solvers (NelderMead),
#   * `OnceDifferentiable` for first-order solvers (GradientDescent, BFGS),
#     with the value/gradient *caching* (`x_f`/`x_df`) that the BFGS line search
#     relies on to reproduce upstream's `f_calls`/`g_calls` accounting, and
#   * a central finite-difference gradient (matching FiniteDiff's default
#     `Val(:central)` step `cbrt(eps(Float64))`) so that the no-gradient
#     `optimize(f, x0, BFGS())` form (`autodiff = :finite` upstream) works
#     (Issue #8059).
#
# Automatic differentiation (ForwardDiff), value caching for `NonDifferentiable`,
# and the `TwiceDifferentiable` / constraint objectives are deliberately deferred.

export AbstractObjective,
    NonDifferentiable,
    OnceDifferentiable,
    value,
    value!,
    value_gradient!,
    gradient,
    gradient!,
    f_calls,
    g_calls

abstract type AbstractObjective end

# Two vectors are considered equal (a cache hit) when they have the same length
# and every element compares equal.  `NaN`-seeded caches therefore never hit on
# the first evaluation, matching upstream's "force a fresh evaluation" behavior.
function _vec_eq(a, b)
    length(a) == length(b) || return false
    for i in eachindex(a)
        a[i] == b[i] || return false
    end
    return true
end

# Central finite-difference step, matching FiniteDiff's default for
# `Val(:central)`: a constant `max(relstep*abs(1), absstep)` with
# `relstep = absstep = cbrt(eps(Float64))`.
const _FD_CENTRAL_STEP = cbrt(eps(Float64))

# Closure factory: return an in-place central finite-difference gradient closure
# that captures the objective `f`.
function _central_difference_gradient(f)
    h = _FD_CENTRAL_STEP
    return function(G, x)
        c1 = Float64[Float64(xi) for xi in x]
        c3 = Float64[Float64(xi) for xi in x]
        for i in eachindex(x)
            xi = Float64(x[i])
            c1[i] = xi + h
            c3[i] = xi - h
            G[i] = (f(c1) - f(c3)) / (2 * h)
            c1[i] = xi
            c3[i] = xi
        end
        return G
    end
end

# ── NonDifferentiable ────────────────────────────────────────────────────────
"""
    NonDifferentiable(f, x0)

Objective wrapper around a scalar-valued function `f` with no derivative
information.  Tracks the number of objective evaluations in `f_calls`.
"""
mutable struct NonDifferentiable <: AbstractObjective
    f
    f_calls::Int
end
NonDifferentiable(f) = NonDifferentiable(f, 0)
NonDifferentiable(f, x0) = NonDifferentiable(f, 0)

function value(obj::NonDifferentiable, x)
    obj.f_calls += 1
    return obj.f(x)
end
value!(obj::NonDifferentiable, x) = value(obj, x)

f_calls(obj::NonDifferentiable) = obj.f_calls
g_calls(obj::NonDifferentiable) = 0

# ── OnceDifferentiable ───────────────────────────────────────────────────────
"""
    OnceDifferentiable(f, g!, x0)
    OnceDifferentiable(f, x0)

Objective wrapper around a scalar-valued function `f` and an in-place gradient
`g!(G, x)`.  When no gradient is supplied, a central finite-difference gradient
is used (upstream's `autodiff = :finite`).

The most recently evaluated value/gradient are cached (keyed on the evaluation
point) so repeated queries at the same `x` — as produced by the line search and
the post-step gradient refresh — do not re-increment `f_calls`/`g_calls`,
mirroring upstream NLSolversBase.
"""
mutable struct OnceDifferentiable <: AbstractObjective
    f
    g!
    f_calls::Int
    g_calls::Int
    F::Float64
    DF::Vector{Float64}
    x_f::Vector{Float64}
    x_df::Vector{Float64}
end

OnceDifferentiable(f, g!, x0) =
    OnceDifferentiable(f, g!, 0, 0, NaN, Float64[], Float64[], Float64[])
# No-gradient form: central finite differences (autodiff = :finite). The gradient
# is a closure capturing `f` (a closure factory).
OnceDifferentiable(f, x0::AbstractArray) =
    OnceDifferentiable(f, _central_difference_gradient(f), 0, 0, NaN, Float64[], Float64[], Float64[])

# `value(obj, x)` always (re-)evaluates and counts, matching upstream's
# non-caching `value` and the existing GradientDescent line search.
function value(obj::OnceDifferentiable, x)
    obj.f_calls += 1
    return obj.f(x)
end

# Cached value: evaluate only when `x` differs from the last value point.
function value!(obj::OnceDifferentiable, x)
    if !_vec_eq(x, obj.x_f)
        obj.f_calls += 1
        obj.F = obj.f(x)
        obj.x_f = Float64[Float64(xi) for xi in x]
    end
    return obj.F
end

# Cached gradient: evaluate only when `x` differs from the last gradient point.
function gradient!(obj::OnceDifferentiable, x)
    if !_vec_eq(x, obj.x_df)
        obj.g_calls += 1
        G = fill(0.0, length(x))
        obj.g!(G, x)
        obj.DF = G
        obj.x_df = Float64[Float64(xi) for xi in x]
    end
    return obj.DF
end

"""
    value_gradient!(obj, x) -> (fx, G)

Evaluate (with caching) the objective and its gradient at `x`, returning the
cached value `fx` and gradient vector `G`.  Each of the value and the gradient is
recomputed only if `x` differs from its respective cache point.
"""
function value_gradient!(obj::OnceDifferentiable, x)
    value!(obj, x)
    gradient!(obj, x)
    return obj.F, obj.DF
end

# Getters (no evaluation, no counting): most recently cached value/gradient.
value(obj::OnceDifferentiable) = obj.F
gradient(obj::OnceDifferentiable) = obj.DF

# Force a fresh (uncached) gradient at `x`; used rarely.
function gradient(obj::OnceDifferentiable, x)
    obj.g_calls += 1
    G = fill(0.0, length(x))
    obj.g!(G, x)
    return G
end

f_calls(obj::OnceDifferentiable) = obj.f_calls
g_calls(obj::OnceDifferentiable) = obj.g_calls

end # module NLSolversBase
