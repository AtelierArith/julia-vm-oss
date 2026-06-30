# Generic truncated univariate distributions (Issue #7325).
#
# Upstream Distributions.jl stores one-sided bounds as `nothing`; this bundled
# subset stores numeric `-Inf` / `Inf` bounds instead. That keeps dispatch simple
# in sjulia while preserving the public `truncated(d; lower=..., upper=...)`
# behavior and the closed-interval semantics.

struct Truncated{D<:Distribution,T<:Real} <: Distribution{Univariate, Continuous}
    untruncated::D
    lower::T
    upper::T
    lcdf::T
    ucdf::T
    tp::T
    logtp::T
end

function _closed_interval_contains(x::Real, lo::Real, hi::Real)
    return lo <= x <= hi
end

function _clamp_to_interval(x, lo, hi)
    if x < lo
        return lo
    elseif x > hi
        return hi
    else
        return x
    end
end

function _truncated_lcdf(d, lower::Real)
    if lower == -Inf
        return 0.0
    end
    return cdf(d, lower)
end

function _truncated_lcdf(d::Bernoulli, lower::Real)
    return lower == -Inf ? 0.0 : cdf(d, lower - 1)
end

function _truncated_lcdf(d::Binomial, lower::Real)
    return lower == -Inf ? 0.0 : cdf(d, lower - 1)
end

function _truncated_lcdf(d::Poisson, lower::Real)
    return lower == -Inf ? 0.0 : cdf(d, lower - 1)
end

function _truncated_lcdf(d::Geometric, lower::Real)
    return lower == -Inf ? 0.0 : cdf(d, lower - 1)
end

function _truncated_lcdf(d::DiscreteUniform, lower::Real)
    return lower == -Inf ? 0.0 : cdf(d, lower - 1)
end

function _truncated_lcdf(d::Categorical, lower::Real)
    return lower == -Inf ? 0.0 : cdf(d, lower - 1)
end

Base.convert(::Type{Truncated}, d::Truncated) = d

function truncated(d, lower::Real, upper::Real)
    l = Float64(lower)
    u = Float64(upper)
    if l > u
        error("truncated: lower bound must be <= upper bound")
    end
    lc = _truncated_lcdf(d, l)
    uc = u == Inf ? 1.0 : cdf(d, u)
    tp = uc - lc
    if tp <= 0.0
        error("truncated: interval has zero probability")
    end
    return Truncated(d, l, u, lc, uc, tp, log(tp))
end

function truncated(d; lower=nothing, upper=nothing)
    if lower === nothing && upper === nothing
        return d
    elseif lower === nothing
        return truncated(d, -Inf, upper)
    elseif upper === nothing
        return truncated(d, lower, Inf)
    else
        return truncated(d, lower, upper)
    end
end

function truncated(d::Truncated, lower::Real, upper::Real)
    return truncated(d.untruncated, max(Float64(lower), d.lower), min(Float64(upper), d.upper))
end

function truncated(d::Truncated; lower=nothing, upper=nothing)
    l = lower === nothing ? d.lower : max(Float64(lower), d.lower)
    u = upper === nothing ? d.upper : min(Float64(upper), d.upper)
    return truncated(d.untruncated, l, u)
end

params(d::Truncated) = (d.untruncated, d.lower, d.upper)
minimum(d::Truncated) = max(minimum(d.untruncated), d.lower)
maximum(d::Truncated) = min(maximum(d.untruncated), d.upper)
insupport(d::Truncated, x::Real) =
    _closed_interval_contains(x, d.lower, d.upper) && insupport(d.untruncated, x)

function pdf(d::Truncated, x::Real)
    result = pdf(d.untruncated, x) / d.tp
    return insupport(d, x) ? result : zero(result)
end

function logpdf(d::Truncated, x::Real)
    result = logpdf(d.untruncated, x) - d.logtp
    return insupport(d, x) ? result : -Inf
end

function cdf(d::Truncated, x::Real)
    if x < d.lower
        return 0.0
    elseif x >= d.upper
        return 1.0
    end
    result = (cdf(d.untruncated, x) - d.lcdf) / d.tp
    return _clamp_to_interval(result, 0.0, 1.0)
end

function quantile(d::Truncated, p::Real)
    x = quantile(d.untruncated, d.lcdf + Float64(p) * d.tp)
    return _clamp_to_interval(x, minimum(d), maximum(d))
end

mode(d::Truncated) = _clamp_to_interval(mode(d.untruncated), minimum(d), maximum(d))
modes(d::Truncated) = [mode(d)]

function mean(d::Truncated)
    n = 256
    s = 0.0
    for i in 1:n
        s += quantile(d, (i - 0.5) / n)
    end
    return s / n
end

_rand_scalar(rng, d::Truncated) = quantile(d, rand(rng))
