# Univariate-distribution plotting recipe (Issue #7262).
#
# Upstream StatsPlots samples a univariate distribution's pdf/pmf over the central
# 99.98% of its mass — `xmin = quantile(d, 0.0001)`, `xmax = quantile(d, 0.9999)`
# — because `minimum(d)` / `maximum(d)` are frequently `±Inf` (Normal, Cauchy, …).
# Continuous distributions become a dense `:line` of the pdf; discrete ones become
# integer-support `:bar` columns of the pmf (upstream uses `:sticks`; the bundled
# Plots backend only has `:line` / `:scatter` / `:bar`, so `:bar` is the closest
# faithful column rendering).
#
# DISPATCH NOTE (Issue #7235): a single `plot(d::Distribution)` method does not
# dispatch reliably in the VM — an *abstract* annotation on a type defined in the
# *Distributions* module fails to match a concrete subtype when called through the
# *Plots* `plot` generic from this third module. (The same limitation forced
# Distributions itself to avoid abstract `d::Distribution` annotations on its own
# module-local generics.) So the recipe is exposed as one thin typed wrapper per
# concrete distribution, each delegating to an untyped helper that does the work.

# Number of pdf/pmf samples for a continuous plot — dense enough for the small
# REPL / iOS canvas, matching the 100-point grid used by `plot(f::Function)`.
const _STATSPLOTS_NPOINTS = 100

# Default x-range covering the central 99.98% of a distribution's mass. Returns a
# `(lo, hi)` tuple of floats; `quantile` is the only field-agnostic way to bound
# heavy-tailed / unbounded supports (`minimum`/`maximum` may be ±Inf).
function _statsplots_xrange(d)
    lo = Float64(quantile(d, 0.0001))
    hi = Float64(quantile(d, 0.9999))
    return (lo, hi)
end

# Continuous recipe: sample the pdf on a uniform grid across the quantile range and
# emit a `:line` series via the existing Plots `plot(x, y)` artifact path.
function _statsplots_continuous_plot(d; title="", aspect_ratio=:auto)
    lo, hi = _statsplots_xrange(d)
    n = _STATSPLOTS_NPOINTS
    step = (hi - lo) / (n - 1)
    xs = Float64[]
    ys = Float64[]
    i = 0
    while i < n
        x = lo + i * step
        push!(xs, x)
        push!(ys, Float64(pdf(d, x)))
        i += 1
    end
    t = isempty(title) ? string(d) : title
    return plot(xs, ys; aspect_ratio=aspect_ratio, title=t)
end

# Discrete recipe: evaluate the pmf at every integer in the quantile range and emit
# a `:bar` series (upstream `:sticks`). The support is integer-valued, so the bar
# centers are the integers `lo:hi`.
function _statsplots_discrete_plot(d; title="", aspect_ratio=:auto)
    lo, hi = _statsplots_xrange(d)
    klo = Int(floor(lo))
    khi = Int(ceil(hi))
    if klo > khi
        klo = khi
    end
    xs = Float64[]
    ys = Float64[]
    k = klo
    while k <= khi
        push!(xs, Float64(k))
        push!(ys, Float64(pdf(d, k)))
        k += 1
    end
    t = isempty(title) ? string(d) : title
    return bar(xs, ys; aspect_ratio=aspect_ratio, title=t)
end

# ── Typed wrappers ───────────────────────────────────────────────────────────
# One per concrete distribution (continuous → pdf line, discrete → pmf bars). Each
# is a thin shim over the untyped helper so the dispatcher only ever matches a
# concrete value type (Issue #7235). `kwargs...` absorbs any extra Plots options
# the caller passes; only `title` / `aspect_ratio` are forwarded to the backend.

# Continuous univariate distributions.
plot(d::Normal; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_continuous_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Uniform; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_continuous_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Exponential; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_continuous_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Gamma; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_continuous_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Beta; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_continuous_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Cauchy; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_continuous_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::LogNormal; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_continuous_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Weibull; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_continuous_plot(d; title=title, aspect_ratio=aspect_ratio)

# Discrete univariate distributions.
plot(d::Bernoulli; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_discrete_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Binomial; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_discrete_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Poisson; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_discrete_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Geometric; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_discrete_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::DiscreteUniform; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_discrete_plot(d; title=title, aspect_ratio=aspect_ratio)
plot(d::Categorical; title="", aspect_ratio=:auto, kwargs...) =
    _statsplots_discrete_plot(d; title=title, aspect_ratio=aspect_ratio)
