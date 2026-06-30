module Distributions

# A pure-Julia subset of Distributions.jl for SubsetJuliaVM (Issue #7178).
# Only the parts needed by educational samples / small notebooks are ported:
# the distribution type hierarchy, a common univariate API, univariate
# continuous and discrete distributions, a minimal multivariate `MvNormal`,
# and simple MLE fitting.

using Random
using SpecialFunctions

import Statistics: mean, var, std, median, quantile, cov

# ── Type hierarchy ──────────────────────────────────────────────────────────
# Mirrors upstream `Distributions.common`: a `VariateForm` describes the shape
# of a sample, a `ValueSupport` its domain.

abstract type VariateForm end
abstract type ValueSupport end

abstract type Univariate    <: VariateForm end
abstract type Multivariate  <: VariateForm end
abstract type Matrixvariate <: VariateForm end

abstract type Discrete   <: ValueSupport end
abstract type Continuous <: ValueSupport end

# NOTE: upstream has `Distribution{F,S} <: Sampleable{F,S}`, but the VM's method
# dispatcher fails to match a *bare* abstract annotation (`d::Distribution`)
# against a concrete subtype when that abstract type itself has a *parametric
# abstract supertype* (Issue #7235). To keep the generic-fallback methods
# (`std(d::Distribution)`, `insupport(d::Distribution, x)`, …) dispatchable,
# `Distribution` is declared without the `Sampleable` supertype here; `Sampleable`
# remains a separate exported abstract type for API compatibility.
abstract type Sampleable{F<:VariateForm, S<:ValueSupport} end
abstract type Distribution{F<:VariateForm, S<:ValueSupport} end

const UnivariateDistribution            = Distribution{Univariate, S} where S
const ContinuousUnivariateDistribution  = Distribution{Univariate, Continuous}
const DiscreteUnivariateDistribution    = Distribution{Univariate, Discrete}
const MultivariateDistribution          = Distribution{Multivariate, S} where S
const ContinuousMultivariateDistribution = Distribution{Multivariate, Continuous}

# ── Exports ─────────────────────────────────────────────────────────────────

export VariateForm, ValueSupport, Univariate, Multivariate, Matrixvariate,
       Discrete, Continuous, Sampleable, Distribution,
       UnivariateDistribution, ContinuousUnivariateDistribution,
       DiscreteUnivariateDistribution, MultivariateDistribution,
       ContinuousMultivariateDistribution
export mean, var, std, median, quantile, cov, mode, params, entropy, scale, rate,
       shape, location, pdf, logpdf, cdf, logcdf, ccdf, logccdf, insupport,
       minimum, maximum, rand!, sampler, modes, skewness, kurtosis, mgf, cf,
       islowerbounded, isupperbounded, isbounded, cquantile, invlogcdf,
       invlogccdf, loglikelihood, kldivergence, Truncated, truncated
# Continuous univariate distributions
export Normal, Uniform, Exponential, Gamma, Beta, TDist, Chisq, FDist,
       Chi, Erlang, InverseGamma, InverseGaussian, Arcsine, TriangularDist,
       SymTriangularDist, Cosine, Semicircle, Kumaraswamy,
       Laplace, Logistic, Rayleigh, Pareto, Gumbel, Frechet, Levy,
       Cauchy, LogNormal, Weibull
# Discrete univariate distributions
export Bernoulli, Binomial, Poisson, Geometric, NegativeBinomial,
       DiscreteUniform, Hypergeometric, BetaBinomial, Skellam, Dirac,
       PoissonBinomial, Categorical
export succprob, failprob, ntrials, span, ncategories, probs, support
# Multivariate distributions
export MvNormal, dim
# Fitting
export fit, fit_mle, suffstats

# ── Generic fallbacks for the common univariate API ─────────────────────────
# Concrete distributions implement `mean`, `var`, `pdf`, `cdf`, `quantile`,
# `minimum`, `maximum`; the rest derive from those.
#
# DISPATCH NOTES (Issue #7235): the VM's method dispatcher does not reliably
# match an *abstract* annotation against a concrete subtype for a *module-local*
# function called from another module. Two consequences shape the code below:
#   * `std` / `median` are imported from `Statistics` (a global generic), where
#     the abstract `d::Distribution` annotation *does* dispatch cross-module, so
#     they keep that annotation (an untyped `d` would clobber `Statistics.std`
#     of arrays).
#   * `logpdf` / `logcdf` / `ccdf` / `logccdf` / `insupport` are brand-new
#     module-local functions; an abstract annotation fails to dispatch
#     cross-module, so they take an *untyped* `d` (matches any distribution and
#     does not collide with any Base/Statistics generic).

std(d::Distribution) = sqrt(var(d))
median(d::Distribution) = quantile(d, 0.5)
logpdf(d, x::Real) = log(pdf(d, x))
logcdf(d, x::Real) = log(cdf(d, x))
ccdf(d, x::Real) = 1.0 - cdf(d, x)
logccdf(d, x::Real) = log(ccdf(d, x))

# `insupport` defaults to the closed interval [minimum(d), maximum(d)].
insupport(d, x::Real) = minimum(d) <= x <= maximum(d)

islowerbounded(d) = minimum(d) > -Inf
isupperbounded(d) = maximum(d) < Inf
isbounded(d) = islowerbounded(d) && isupperbounded(d)

modes(d) = [mode(d)]
kurtosis(d, correction::Bool) = correction ? kurtosis(d) : kurtosis(d) + 3.0

mgf(d, t::Real) = error("mgf: unsupported distribution")
cf(d, t::Real) = error("cf: unsupported distribution")

cquantile(d, p::Real) = quantile(d, 1.0 - Float64(p))
invlogcdf(d, lp::Real) = quantile(d, exp(Float64(lp)))
invlogccdf(d, lp::Real) = quantile(d, -expm1(Float64(lp)))

loglikelihood(d, x::Real) = logpdf(d, x)
function loglikelihood(d, x)
    s = 0.0
    for v in x
        s += logpdf(d, v)
    end
    return s
end

kldivergence(p, q) = error("kldivergence: unsupported distribution pair")

include("univariate/continuous.jl")
include("univariate/discrete.jl")
include("truncate.jl")
include("multivariate/mvnormal.jl")
include("fit.jl")

# ── Generic sampling API ──────────────────────────────────────────────────────
# Upstream Distributions routes batch sampling through `sampler(d)`. The subset
# keeps the default sampler as `d` itself; specialized samplers can be added later
# without changing the public `rand`/`rand!` API (Issue #7323).

sampler(d::Distribution) = d

function _fill_rand!(rng::AbstractRNG, d::Distribution, A)
    for i in 1:length(A)
        A[i] = _rand_scalar(rng, d)
    end
    return A
end

function _rand_array_float(rng, d, dim1::Integer, dims::Integer...)
    out = zeros(Float64, Int(dim1), dims...)
    return _fill_rand!(rng, d, out)
end

function _rand_array_int(rng, d, dim1::Integer, dims::Integer...)
    out = zeros(Int64, Int(dim1), dims...)
    return _fill_rand!(rng, d, out)
end

rand(rng::AbstractRNG, d::Distribution) = _rand_scalar(rng, d)
rand(d::Distribution) = _rand_scalar(Random.default_rng(), sampler(d))

rand(rng, d::Normal) = _rand_scalar(rng, d)
rand(rng, d::Uniform) = _rand_scalar(rng, d)
rand(rng, d::Exponential) = _rand_scalar(rng, d)
rand(rng, d::Gamma) = _rand_scalar(rng, d)
rand(rng, d::Beta) = _rand_scalar(rng, d)
rand(rng, d::TDist) = _rand_scalar(rng, d)
rand(rng, d::Chisq) = _rand_scalar(rng, d)
rand(rng, d::FDist) = _rand_scalar(rng, d)
rand(rng, d::Chi) = _rand_scalar(rng, d)
rand(rng, d::Erlang) = _rand_scalar(rng, d)
rand(rng, d::InverseGamma) = _rand_scalar(rng, d)
rand(rng, d::InverseGaussian) = _rand_scalar(rng, d)
rand(rng, d::Arcsine) = _rand_scalar(rng, d)
rand(rng, d::TriangularDist) = _rand_scalar(rng, d)
rand(rng, d::SymTriangularDist) = _rand_scalar(rng, d)
rand(rng, d::Cosine) = _rand_scalar(rng, d)
rand(rng, d::Semicircle) = _rand_scalar(rng, d)
rand(rng, d::Kumaraswamy) = _rand_scalar(rng, d)
rand(rng, d::Laplace) = _rand_scalar(rng, d)
rand(rng, d::Logistic) = _rand_scalar(rng, d)
rand(rng, d::Rayleigh) = _rand_scalar(rng, d)
rand(rng, d::Pareto) = _rand_scalar(rng, d)
rand(rng, d::Gumbel) = _rand_scalar(rng, d)
rand(rng, d::Frechet) = _rand_scalar(rng, d)
rand(rng, d::Levy) = _rand_scalar(rng, d)
rand(rng, d::Cauchy) = _rand_scalar(rng, d)
rand(rng, d::LogNormal) = _rand_scalar(rng, d)
rand(rng, d::Weibull) = _rand_scalar(rng, d)
rand(rng, d::Bernoulli) = _rand_scalar(rng, d)
rand(rng, d::Binomial) = _rand_scalar(rng, d)
rand(rng, d::Poisson) = _rand_scalar(rng, d)
rand(rng, d::Geometric) = _rand_scalar(rng, d)
rand(rng, d::NegativeBinomial) = _rand_scalar(rng, d)
rand(rng, d::DiscreteUniform) = _rand_scalar(rng, d)
rand(rng, d::Hypergeometric) = _rand_scalar(rng, d)
rand(rng, d::BetaBinomial) = _rand_scalar(rng, d)
rand(rng, d::Skellam) = _rand_scalar(rng, d)
rand(rng, d::Dirac) = _rand_scalar(rng, d)
rand(rng, d::PoissonBinomial) = _rand_scalar(rng, d)
rand(rng, d::Categorical) = _rand_scalar(rng, d)
rand(rng, d::Truncated) = _rand_scalar(rng, d)

rand(d::Normal, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Uniform, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Exponential, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Gamma, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Beta, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::TDist, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Chisq, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::FDist, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Chi, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Erlang, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::InverseGamma, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::InverseGaussian, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Arcsine, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::TriangularDist, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::SymTriangularDist, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Cosine, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Semicircle, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Kumaraswamy, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Laplace, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Logistic, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Rayleigh, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Pareto, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Gumbel, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Frechet, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Levy, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Cauchy, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::LogNormal, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Weibull, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)
rand(d::Bernoulli, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::Binomial, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::Poisson, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::Geometric, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::NegativeBinomial, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::DiscreteUniform, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::Hypergeometric, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::BetaBinomial, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::Skellam, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::Dirac, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::PoissonBinomial, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::Categorical, dim1::Integer, dims::Integer...) =
    _rand_array_int(Random.default_rng(), d, dim1, dims...)
rand(d::Truncated, dim1::Integer, dims::Integer...) =
    _rand_array_float(Random.default_rng(), d, dim1, dims...)

rand(rng, d::Normal, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Uniform, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Exponential, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Gamma, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Beta, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::TDist, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Chisq, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::FDist, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Chi, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Erlang, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::InverseGamma, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::InverseGaussian, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Arcsine, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::TriangularDist, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::SymTriangularDist, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Cosine, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Semicircle, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Kumaraswamy, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Laplace, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Logistic, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Rayleigh, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Pareto, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Gumbel, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Frechet, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Levy, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Cauchy, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::LogNormal, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Weibull, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)
rand(rng, d::Bernoulli, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::Binomial, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::Poisson, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::Geometric, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::NegativeBinomial, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::DiscreteUniform, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::Hypergeometric, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::BetaBinomial, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::Skellam, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::Dirac, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::PoissonBinomial, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::Categorical, dim1::Integer, dims::Integer...) =
    _rand_array_int(rng, d, dim1, dims...)
rand(rng, d::Truncated, dim1::Integer, dims::Integer...) =
    _rand_array_float(rng, d, dim1, dims...)

function rand(d::ContinuousUnivariateDistribution, dim1::Integer, dims::Integer...)
    out = zeros(Float64, Int(dim1), dims...)
    return _fill_rand!(Random.default_rng(), sampler(d), out)
end

function rand(d::DiscreteUnivariateDistribution, dim1::Integer, dims::Integer...)
    out = zeros(Int64, Int(dim1), dims...)
    return _fill_rand!(Random.default_rng(), sampler(d), out)
end

function rand(rng::AbstractRNG, d::ContinuousUnivariateDistribution, dim1::Integer, dims::Integer...)
    return _rand_array_float(rng, sampler(d), dim1, dims...)
end

function rand(rng::AbstractRNG, d::DiscreteUnivariateDistribution, dim1::Integer, dims::Integer...)
    return _rand_array_int(rng, sampler(d), dim1, dims...)
end

rand!(d::Distribution, A) = _fill_rand!(Random.default_rng(), sampler(d), A)

function rand!(rng::AbstractRNG, d::Distribution, A)
    return _fill_rand!(rng, sampler(d), A)
end

end # module Distributions
