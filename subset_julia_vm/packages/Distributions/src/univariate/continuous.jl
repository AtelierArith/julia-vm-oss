# Univariate continuous distributions (Issue #7178, Phase 2).
#
# Each distribution implements the common API declared in Distributions.jl:
# params, mean, var, (mode), pdf, cdf, quantile, minimum, maximum, rand.
# Sampling supports both the global RNG (`rand(d)`) and explicit RNGs
# (`rand(rng, d)`) through the shared generic API in Distributions.jl.

# Precomputed literals (avoid calling sqrt/log during module-load const init,
# which cannot resolve Base's module-private log helpers at that point).
const _sqrt2 = 1.4142135623730951   # sqrt(2)
const _sqrt2π = 2.5066282746310002  # sqrt(2π)
const _log2π = 1.8378770664093453   # log(2π)
const _log2 = 0.6931471805599453    # log(2)
const _sqrtπ = 1.7724538509055160   # sqrt(π)
const _eulergamma = 0.5772156649015329
const _gumbel_skewness = 1.1395470994046486

function _rand_open01(rng)
    u = rand(rng)
    while u == 0.0
        u = rand(rng)
    end
    return u
end

_randexp_unit(rng) = -log(_rand_open01(rng))

function _digamma_approx(x::Real)
    y = Float64(x)
    result = 0.0
    while y < 8.0
        result -= 1.0 / y
        y += 1.0
    end
    inv = 1.0 / y
    inv2 = inv * inv
    return result + log(y) - 0.5 * inv -
           inv2 * (1.0 / 12.0 - inv2 * (1.0 / 120.0 - inv2 / 252.0))
end

_std_norm_cdf(x::Real) = 0.5 * erfc(-Float64(x) / _sqrt2)
_clamp01(x::Real) = x < 0.0 ? 0.0 : (x > 1.0 ? 1.0 : x)

# Standard-normal inverse CDF via Acklam's rational approximation
# (relative error < 1.15e-9), used by Normal/LogNormal quantiles.
function _norminvcdf(p::Float64)
    if p <= 0.0 || p >= 1.0
        if p == 0.0
            return -Inf
        elseif p == 1.0
            return Inf
        else
            error("norminvcdf requires 0 <= p <= 1")
        end
    end
    a1 = -3.969683028665376e+01; a2 = 2.209460984245205e+02
    a3 = -2.759285104469687e+02; a4 = 1.383577518672690e+02
    a5 = -3.066479806614716e+01; a6 = 2.506628277459239e+00
    b1 = -5.447609879822406e+01; b2 = 1.615858368580409e+02
    b3 = -1.556989798598866e+02; b4 = 6.680131188771972e+01
    b5 = -1.328068155288572e+01
    c1 = -7.784894002430293e-03; c2 = -3.223964580411365e-01
    c3 = -2.400758277161838e+00; c4 = -2.549732539343734e+00
    c5 = 4.374664141464968e+00;  c6 = 2.938163982698783e+00
    d1 = 7.784695709041462e-03;  d2 = 3.224671290700398e-01
    d3 = 2.445134137142996e+00;  d4 = 3.754408661907416e+00
    plow = 0.02425
    phigh = 1.0 - plow
    if p < plow
        q = sqrt(-2.0 * log(p))
        return (((((c1*q+c2)*q+c3)*q+c4)*q+c5)*q+c6) /
               ((((d1*q+d2)*q+d3)*q+d4)*q+1.0)
    elseif p <= phigh
        q = p - 0.5
        r = q * q
        return (((((a1*r+a2)*r+a3)*r+a4)*r+a5)*r+a6)*q /
               (((((b1*r+b2)*r+b3)*r+b4)*r+b5)*r+1.0)
    else
        q = sqrt(-2.0 * log(1.0 - p))
        return -(((((c1*q+c2)*q+c3)*q+c4)*q+c5)*q+c6) /
                ((((d1*q+d2)*q+d3)*q+d4)*q+1.0)
    end
end

# Generic monotone-CDF quantile by bisection, for distributions without a
# closed-form inverse CDF (Gamma, Beta).
function _bisect_quantile(d, q::Real, lo::Float64, hi::Float64)
    q = Float64(q)
    if q <= 0.0
        return lo
    elseif q >= 1.0
        return hi
    end
    for _ in 1:200
        mid = 0.5 * (lo + hi)
        if cdf(d, mid) < q
            lo = mid
        else
            hi = mid
        end
        if hi - lo < 1e-12 * (abs(hi) + abs(lo) + 1.0)
            break
        end
    end
    return 0.5 * (lo + hi)
end

# ── Normal ──────────────────────────────────────────────────────────────────

struct Normal{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    σ::T
end

function Normal(μ::Real, σ::Real)
    if σ < 0
        throw(ArgumentError("Normal: the condition σ >= 0 is not satisfied."))
    end
    m, s = promote(float(μ), float(σ))
    return Normal{typeof(m)}(m, s)
end
Normal(μ::Real) = Normal(μ, 1.0)
Normal() = Normal(0.0, 1.0)

params(d::Normal) = (d.μ, d.σ)
mean(d::Normal) = d.μ
median(d::Normal) = d.μ
mode(d::Normal) = d.μ
var(d::Normal) = d.σ^2
std(d::Normal) = d.σ
skewness(d::Normal) = 0.0
kurtosis(d::Normal) = 0.0
scale(d::Normal) = d.σ
entropy(d::Normal) = (_log2π + 1.0) / 2.0 + log(d.σ)
minimum(d::Normal) = -Inf
maximum(d::Normal) = Inf

pdf(d::Normal, x::Real) = exp(-0.5 * ((x - d.μ) / d.σ)^2) / (d.σ * _sqrt2π)
function logpdf(d::Normal, x::Real)
    z = (x - d.μ) / d.σ
    return -0.5 * z^2 - log(d.σ) - 0.5 * _log2π
end
cdf(d::Normal, x::Real) = 0.5 * erfc(-(x - d.μ) / (d.σ * _sqrt2))
quantile(d::Normal, q::Real) = d.μ + d.σ * _norminvcdf(Float64(q))
mgf(d::Normal, t::Real) = exp(t * d.μ + d.σ^2 * t^2 / 2.0)
cf(d::Normal, t::Real) = exp(-d.σ^2 * t^2 / 2.0) * cis(t * d.μ)
function kldivergence(p::Normal, q::Normal)
    vp = var(p)
    vq = var(q)
    return log(q.σ / p.σ) + (vp + (p.μ - q.μ)^2) / (2.0 * vq) - 0.5
end
rand(d::Normal) = d.μ + d.σ * randn()
_rand_scalar(rng, d::Normal) = d.μ + d.σ * randn(rng)

# ── Uniform ─────────────────────────────────────────────────────────────────

struct Uniform{T<:Real} <: Distribution{Univariate, Continuous}
    a::T
    b::T
end

function Uniform(a::Real, b::Real)
    if a >= b
        throw(ArgumentError("Uniform: the condition a < b is not satisfied."))
    end
    x, y = promote(float(a), float(b))
    return Uniform{typeof(x)}(x, y)
end
Uniform() = Uniform(0.0, 1.0)

params(d::Uniform) = (d.a, d.b)
mean(d::Uniform) = (d.a + d.b) / 2.0
median(d::Uniform) = (d.a + d.b) / 2.0
mode(d::Uniform) = mean(d)
modes(d::Uniform) = Float64[]
var(d::Uniform) = (d.b - d.a)^2 / 12.0
skewness(d::Uniform) = 0.0
kurtosis(d::Uniform) = -6.0 / 5.0
entropy(d::Uniform) = log(d.b - d.a)
minimum(d::Uniform) = d.a
maximum(d::Uniform) = d.b

pdf(d::Uniform, x::Real) = (d.a <= x <= d.b) ? 1.0 / (d.b - d.a) : 0.0
function cdf(d::Uniform, x::Real)
    if x < d.a
        return 0.0
    elseif x > d.b
        return 1.0
    else
        return (x - d.a) / (d.b - d.a)
    end
end
quantile(d::Uniform, q::Real) = d.a + Float64(q) * (d.b - d.a)
cquantile(d::Uniform, p::Real) = d.b + Float64(p) * (d.a - d.b)
function mgf(d::Uniform, t::Real)
    u = (d.b - d.a) * Float64(t) / 2.0
    if u == 0.0
        return 1.0
    end
    v = (d.a + d.b) * Float64(t) / 2.0
    return exp(v) * (sinh(u) / u)
end
function cf(d::Uniform, t::Real)
    u = (d.b - d.a) * Float64(t) / 2.0
    if u == 0.0
        return Complex{Float64}(1.0, 0.0)
    end
    v = (d.a + d.b) * Float64(t) / 2.0
    return cis(v) * (sin(u) / u)
end
rand(d::Uniform) = d.a + (d.b - d.a) * rand()
_rand_scalar(rng, d::Uniform) = d.a + (d.b - d.a) * rand(rng)

# ── Exponential (scale parameterization θ) ──────────────────────────────────

struct Exponential{T<:Real} <: Distribution{Univariate, Continuous}
    θ::T
end

function Exponential(θ::Real)
    if θ <= 0
        throw(ArgumentError("Exponential: the condition θ > 0 is not satisfied."))
    end
    t = float(θ)
    return Exponential{typeof(t)}(t)
end
Exponential() = Exponential(1.0)

params(d::Exponential) = (d.θ,)
scale(d::Exponential) = d.θ
rate(d::Exponential) = 1.0 / d.θ
mean(d::Exponential) = d.θ
median(d::Exponential) = d.θ * log(2.0)
mode(d::Exponential) = 0.0
var(d::Exponential) = d.θ^2
skewness(d::Exponential) = 2.0
kurtosis(d::Exponential) = 6.0
entropy(d::Exponential) = 1.0 + log(d.θ)
minimum(d::Exponential) = 0.0
maximum(d::Exponential) = Inf

pdf(d::Exponential, x::Real) = x < 0.0 ? 0.0 : exp(-x / d.θ) / d.θ
cdf(d::Exponential, x::Real) = x < 0.0 ? 0.0 : 1.0 - exp(-x / d.θ)
quantile(d::Exponential, q::Real) = -d.θ * log(1.0 - Float64(q))
cquantile(d::Exponential, p::Real) = -d.θ * log(Float64(p))
invlogcdf(d::Exponential, lp::Real) = -d.θ * log(1.0 - exp(Float64(lp)))
invlogccdf(d::Exponential, lp::Real) = -d.θ * Float64(lp)
mgf(d::Exponential, t::Real) = 1.0 / (1.0 - Float64(t) * d.θ)
cf(d::Exponential, t::Real) = 1.0 / (1.0 - im * Float64(t) * d.θ)
function kldivergence(p::Exponential, q::Exponential)
    r = q.θ / p.θ
    return r - log(r) - 1.0
end
rand(d::Exponential) = -d.θ * log(1.0 - rand())
_rand_scalar(rng, d::Exponential) = -d.θ * log(1.0 - rand(rng))

# ── Gamma (shape α, scale θ) ────────────────────────────────────────────────

struct Gamma{T<:Real} <: Distribution{Univariate, Continuous}
    α::T
    θ::T
end

function Gamma(α::Real, θ::Real)
    if α <= 0 || θ <= 0
        throw(ArgumentError("Gamma: the conditions α > 0 and θ > 0 are not satisfied."))
    end
    a, t = promote(float(α), float(θ))
    return Gamma{typeof(a)}(a, t)
end
Gamma(α::Real) = Gamma(α, 1.0)
Gamma() = Gamma(1.0, 1.0)

params(d::Gamma) = (d.α, d.θ)
shape(d::Gamma) = d.α
scale(d::Gamma) = d.θ
rate(d::Gamma) = 1.0 / d.θ
mean(d::Gamma) = d.α * d.θ
var(d::Gamma) = d.α * d.θ^2
mode(d::Gamma) = d.α >= 1.0 ? (d.α - 1.0) * d.θ : 0.0
skewness(d::Gamma) = 2.0 / sqrt(d.α)
kurtosis(d::Gamma) = 6.0 / d.α
minimum(d::Gamma) = 0.0
maximum(d::Gamma) = Inf

function pdf(d::Gamma, x::Real)
    x = Float64(x)
    if x < 0.0
        return 0.0
    end
    if x == 0.0
        return d.α < 1.0 ? Inf : (d.α == 1.0 ? 1.0 / d.θ : 0.0)
    end
    return exp((d.α - 1.0) * log(x) - x / d.θ - loggamma(d.α) - d.α * log(d.θ))
end
cdf(d::Gamma, x::Real) = x <= 0.0 ? 0.0 : gamma_inc(d.α, Float64(x) / d.θ)
# Upper bracket = mean + 20·std + slack, computed directly from the fields.
# (Calling the imported abstract generics `mean(d)`/`std(d)` from inside this
# module's own method body fails to compile under the VM dispatcher — Issue
# #7235 — so the moments are inlined here.)
quantile(d::Gamma, q::Real) =
    _bisect_quantile(d, q, 0.0, d.α * d.θ + 20.0 * sqrt(d.α) * d.θ + 1000.0)
mgf(d::Gamma, t::Real) = (1.0 - Float64(t) * d.θ)^(-d.α)
cf(d::Gamma, t::Real) = (1.0 - im * Float64(t) * d.θ)^(-d.α)

# Marsaglia & Tsang (2000) gamma sampler, scaled by θ.
function _rand_gamma_shape(rng::AbstractRNG, α::Float64)
    if α < 1.0
        # Boost: Gamma(α) = Gamma(α+1) * U^(1/α)
        u = rand(rng)
        return _rand_gamma_shape(rng, α + 1.0) * u^(1.0 / α)
    end
    d = α - 1.0 / 3.0
    c = 1.0 / sqrt(9.0 * d)
    while true
        x = randn(rng)
        v = (1.0 + c * x)^3
        if v <= 0.0
            continue
        end
        u = rand(rng)
        if log(u) < 0.5 * x^2 + d - d * v + d * log(v)
            return d * v
        end
    end
end
_rand_gamma_shape(α::Float64) = _rand_gamma_shape(Random.default_rng(), α)
rand(d::Gamma) = d.θ * _rand_gamma_shape(Float64(d.α))
_rand_scalar(rng, d::Gamma) = d.θ * _rand_gamma_shape(rng, Float64(d.α))

# ── Classical test distributions ───────────────────────────────────────────

struct Chisq{T<:Real} <: Distribution{Univariate, Continuous}
    ν::T
end

function Chisq(ν::Real)
    if ν <= 0
        throw(ArgumentError("Chisq: the condition ν > 0 is not satisfied."))
    end
    v = float(ν)
    return Chisq{typeof(v)}(v)
end

params(d::Chisq) = (d.ν,)
dof(d::Chisq) = d.ν
mean(d::Chisq) = d.ν
var(d::Chisq) = 2.0 * d.ν
mode(d::Chisq) = d.ν > 2.0 ? d.ν - 2.0 : 0.0
skewness(d::Chisq) = sqrt(8.0 / d.ν)
kurtosis(d::Chisq) = 12.0 / d.ν
minimum(d::Chisq) = 0.0
maximum(d::Chisq) = Inf

function logpdf(d::Chisq, x::Real)
    x = Float64(x)
    if x < 0.0
        return -Inf
    elseif x == 0.0
        if d.ν == 2.0
            return -log(2.0)
        elseif d.ν < 2.0
            return Inf
        else
            return -Inf
        end
    end
    h = d.ν / 2.0
    return (h - 1.0) * log(x) - x / 2.0 - h * log(2.0) - loggamma(h)
end
pdf(d::Chisq, x::Real) = exp(logpdf(d, x))
cdf(d::Chisq, x::Real) = Float64(x) <= 0.0 ? 0.0 : gamma_inc(d.ν / 2.0, Float64(x) / 2.0)
quantile(d::Chisq, q::Real) =
    _bisect_quantile(d, q, 0.0, d.ν + 20.0 * sqrt(2.0 * d.ν) + 1000.0)
rand(d::Chisq) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Chisq) = 2.0 * _rand_gamma_shape(rng, Float64(d.ν) / 2.0)

struct TDist{T<:Real} <: Distribution{Univariate, Continuous}
    ν::T
end

function TDist(ν::Real)
    if ν <= 0
        throw(ArgumentError("TDist: the condition ν > 0 is not satisfied."))
    end
    v = float(ν)
    return TDist{typeof(v)}(v)
end

params(d::TDist) = (d.ν,)
dof(d::TDist) = d.ν
mean(d::TDist) = d.ν > 1.0 ? 0.0 : NaN
median(d::TDist) = 0.0
mode(d::TDist) = 0.0
function var(d::TDist)
    if d.ν > 2.0
        return d.ν / (d.ν - 2.0)
    elseif d.ν > 1.0
        return Inf
    else
        return NaN
    end
end
skewness(d::TDist) = d.ν > 3.0 ? 0.0 : NaN
kurtosis(d::TDist) = d.ν > 4.0 ? 6.0 / (d.ν - 4.0) : (d.ν > 2.0 ? Inf : NaN)
minimum(d::TDist) = -Inf
maximum(d::TDist) = Inf

function logpdf(d::TDist, x::Real)
    ν = Float64(d.ν)
    z = Float64(x)
    return -0.5 * log(ν) - lbeta(0.5, ν / 2.0) -
           ((ν + 1.0) / 2.0) * log(1.0 + z * z / ν)
end
pdf(d::TDist, x::Real) = exp(logpdf(d, x))
function cdf(d::TDist, x::Real)
    z = Float64(x)
    if z == 0.0
        return 0.5
    end
    ν = Float64(d.ν)
    t = ν / (ν + z * z)
    ib = beta_inc(ν / 2.0, 0.5, t)
    return z > 0.0 ? 1.0 - 0.5 * ib : 0.5 * ib
end
function quantile(d::TDist, q::Real)
    p = Float64(q)
    if p <= 0.0
        return -Inf
    elseif p >= 1.0
        return Inf
    elseif p == 0.5
        return 0.0
    elseif p < 0.5
        return -quantile(d, 1.0 - p)
    end
    hi = 1.0
    while cdf(d, hi) < p
        hi *= 2.0
    end
    return _bisect_quantile(d, p, 0.0, hi)
end
rand(d::TDist) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::TDist)
    z = randn(rng)
    c = _rand_scalar(rng, Chisq(d.ν))
    return z / sqrt(c / d.ν)
end

struct FDist{T<:Real} <: Distribution{Univariate, Continuous}
    ν1::T
    ν2::T
end

function FDist(ν1::Real, ν2::Real)
    if ν1 <= 0 || ν2 <= 0
        throw(ArgumentError("FDist: the conditions ν1 > 0 and ν2 > 0 are not satisfied."))
    end
    a, b = promote(float(ν1), float(ν2))
    return FDist{typeof(a)}(a, b)
end

params(d::FDist) = (d.ν1, d.ν2)
mean(d::FDist) = d.ν2 > 2.0 ? d.ν2 / (d.ν2 - 2.0) : NaN
function var(d::FDist)
    if d.ν2 <= 4.0
        return NaN
    end
    return 2.0 * d.ν2^2 * (d.ν1 + d.ν2 - 2.0) /
           (d.ν1 * (d.ν2 - 2.0)^2 * (d.ν2 - 4.0))
end
mode(d::FDist) =
    d.ν1 > 2.0 ? ((d.ν1 - 2.0) / d.ν1) * (d.ν2 / (d.ν2 + 2.0)) : 0.0
minimum(d::FDist) = 0.0
maximum(d::FDist) = Inf

function logpdf(d::FDist, x::Real)
    x = Float64(x)
    if x <= 0.0
        return -Inf
    end
    h1 = d.ν1 / 2.0
    h2 = d.ν2 / 2.0
    return h1 * log(d.ν1 / d.ν2) + (h1 - 1.0) * log(x) -
           lbeta(h1, h2) - (h1 + h2) * log(1.0 + d.ν1 * x / d.ν2)
end
pdf(d::FDist, x::Real) = exp(logpdf(d, x))
function cdf(d::FDist, x::Real)
    x = Float64(x)
    if x <= 0.0
        return 0.0
    end
    return beta_inc(d.ν1 / 2.0, d.ν2 / 2.0, (d.ν1 * x) / (d.ν1 * x + d.ν2))
end
function quantile(d::FDist, q::Real)
    p = Float64(q)
    if p <= 0.0
        return 0.0
    elseif p >= 1.0
        return Inf
    end
    hi = d.ν2 > 2.0 ? max(1.0, mean(d) * 2.0) : 1.0
    while cdf(d, hi) < p
        hi *= 2.0
    end
    return _bisect_quantile(d, p, 0.0, hi)
end
rand(d::FDist) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::FDist)
    x = _rand_scalar(rng, Chisq(d.ν1)) / d.ν1
    y = _rand_scalar(rng, Chisq(d.ν2)) / d.ν2
    return x / y
end

# ── Continuous expansion distributions ─────────────────────────────────────

struct Chi{T<:Real} <: Distribution{Univariate, Continuous}
    ν::T
end

function Chi(ν::Real)
    if ν <= 0
        throw(ArgumentError("Chi: the condition ν > 0 is not satisfied."))
    end
    v = float(ν)
    return Chi{typeof(v)}(v)
end

params(d::Chi) = (d.ν,)
dof(d::Chi) = d.ν
mean(d::Chi) = (h = d.ν / 2.0; _sqrt2 * exp(loggamma(h + 0.5) - loggamma(h)))
var(d::Chi) = d.ν - mean(d)^2
function mode(d::Chi)
    if d.ν < 1.0
        error("Chi distribution has no mode when ν < 1")
    end
    return sqrt(d.ν - 1.0)
end
function skewness(d::Chi)
    μ = mean(d)
    σ2 = var(d)
    σ = sqrt(σ2)
    return (μ / (σ2 * σ)) * (1.0 - 2.0 * σ2)
end
function kurtosis(d::Chi)
    μ = mean(d)
    σ2 = var(d)
    σ = sqrt(σ2)
    γ = (μ / (σ2 * σ)) * (1.0 - 2.0 * σ2)
    return (2.0 / σ2) * (1.0 - μ * σ * γ - σ2)
end
entropy(d::Chi) =
    loggamma(d.ν / 2.0) - _log2 / 2.0 -
    ((d.ν - 1.0) / 2.0) * _digamma_approx(d.ν / 2.0) + d.ν / 2.0
minimum(d::Chi) = 0.0
maximum(d::Chi) = Inf

function logpdf(d::Chi, x::Real)
    x = Float64(x)
    if x < 0.0
        return -Inf
    elseif x == 0.0
        if d.ν == 1.0
            return 0.5 * _log2 - loggamma(0.5)
        elseif d.ν < 1.0
            return Inf
        else
            return -Inf
        end
    end
    return (1.0 - d.ν / 2.0) * _log2 - loggamma(d.ν / 2.0) +
           (d.ν - 1.0) * log(x) - x^2 / 2.0
end
pdf(d::Chi, x::Real) = exp(logpdf(d, x))
cdf(d::Chi, x::Real) = Float64(x) <= 0.0 ? 0.0 : cdf(Chisq(d.ν), Float64(x)^2)
quantile(d::Chi, q::Real) = sqrt(quantile(Chisq(d.ν), q))
rand(d::Chi) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Chi) = sqrt(_rand_scalar(rng, Chisq(d.ν)))

struct Erlang{T<:Real} <: Distribution{Univariate, Continuous}
    α::Int
    θ::T
end

function Erlang(α::Real, θ::Real)
    if α < 0 || α != floor(α) || θ <= 0
        throw(ArgumentError("Erlang: α must be a nonnegative integer and θ > 0."))
    end
    t = float(θ)
    return Erlang{typeof(t)}(Int(α), t)
end
Erlang(α::Real) = Erlang(α, 1.0)
Erlang(α::Integer) = Erlang(α, 1.0)
Erlang() = Erlang(1, 1.0)

params(d::Erlang) = (d.α, d.θ)
shape(d::Erlang) = d.α
scale(d::Erlang) = d.θ
rate(d::Erlang) = 1.0 / d.θ
mean(d::Erlang) = d.α * d.θ
var(d::Erlang) = d.α * d.θ^2
skewness(d::Erlang) = 2.0 / sqrt(d.α)
kurtosis(d::Erlang) = 6.0 / d.α
function mode(d::Erlang)
    if d.α < 1
        error("Erlang has no mode when α < 1")
    end
    return d.θ * (d.α - 1)
end
entropy(d::Erlang) =
    d.α + loggamma(Float64(d.α)) +
    (1.0 - d.α) * _digamma_approx(Float64(d.α)) + log(d.θ)
minimum(d::Erlang) = 0.0
maximum(d::Erlang) = Inf

_as_gamma(d::Erlang) = Gamma(Float64(d.α), d.θ)
pdf(d::Erlang, x::Real) = pdf(_as_gamma(d), x)
logpdf(d::Erlang, x::Real) = logpdf(_as_gamma(d), x)
cdf(d::Erlang, x::Real) = cdf(_as_gamma(d), x)
quantile(d::Erlang, q::Real) = quantile(_as_gamma(d), q)
mgf(d::Erlang, t::Real) = (1.0 - Float64(t) * d.θ)^(-d.α)
cf(d::Erlang, t::Real) = (1.0 - im * Float64(t) * d.θ)^(-d.α)
rand(d::Erlang) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Erlang) = _rand_scalar(rng, _as_gamma(d))

struct InverseGamma{T<:Real} <: Distribution{Univariate, Continuous}
    α::T
    θ::T
end

function InverseGamma(α::Real, θ::Real)
    if α <= 0 || θ <= 0
        throw(ArgumentError("InverseGamma: the conditions α > 0 and θ > 0 are not satisfied."))
    end
    a, t = promote(float(α), float(θ))
    return InverseGamma{typeof(a)}(a, t)
end
InverseGamma(α::Real) = InverseGamma(α, 1.0)
InverseGamma() = InverseGamma(1.0, 1.0)

params(d::InverseGamma) = (d.α, d.θ)
shape(d::InverseGamma) = d.α
scale(d::InverseGamma) = d.θ
rate(d::InverseGamma) = 1.0 / d.θ
mean(d::InverseGamma) = d.α > 1.0 ? d.θ / (d.α - 1.0) : Inf
mode(d::InverseGamma) = d.θ / (d.α + 1.0)
function var(d::InverseGamma)
    return d.α > 2.0 ? d.θ^2 / ((d.α - 1.0)^2 * (d.α - 2.0)) : Inf
end
skewness(d::InverseGamma) = d.α > 3.0 ? 4.0 * sqrt(d.α - 2.0) / (d.α - 3.0) : NaN
kurtosis(d::InverseGamma) =
    d.α > 4.0 ? (30.0 * d.α - 66.0) / ((d.α - 3.0) * (d.α - 4.0)) : NaN
entropy(d::InverseGamma) =
    d.α + loggamma(d.α) - (1.0 + d.α) * _digamma_approx(d.α) + log(d.θ)
minimum(d::InverseGamma) = 0.0
maximum(d::InverseGamma) = Inf

_inverse_gamma_invd(d::InverseGamma) = Gamma(d.α, 1.0 / d.θ)
function logpdf(d::InverseGamma, x::Real)
    x = Float64(x)
    if x <= 0.0
        return -Inf
    end
    return d.α * log(d.θ) - loggamma(d.α) - (d.α + 1.0) * log(x) - d.θ / x
end
pdf(d::InverseGamma, x::Real) = exp(logpdf(d, x))
function cdf(d::InverseGamma, x::Real)
    x = Float64(x)
    return x <= 0.0 ? 0.0 : 1.0 - cdf(_inverse_gamma_invd(d), 1.0 / x)
end
function quantile(d::InverseGamma, q::Real)
    p = Float64(q)
    if p <= 0.0
        return 0.0
    elseif p >= 1.0
        return Inf
    end
    return 1.0 / quantile(_inverse_gamma_invd(d), 1.0 - p)
end
rand(d::InverseGamma) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::InverseGamma) = 1.0 / _rand_scalar(rng, _inverse_gamma_invd(d))

struct InverseGaussian{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    λ::T
end

function InverseGaussian(μ::Real, λ::Real)
    if μ <= 0 || λ <= 0
        throw(ArgumentError("InverseGaussian: the conditions μ > 0 and λ > 0 are not satisfied."))
    end
    m, l = promote(float(μ), float(λ))
    return InverseGaussian{typeof(m)}(m, l)
end
InverseGaussian(μ::Real) = InverseGaussian(μ, 1.0)
InverseGaussian() = InverseGaussian(1.0, 1.0)

params(d::InverseGaussian) = (d.μ, d.λ)
shape(d::InverseGaussian) = d.λ
mean(d::InverseGaussian) = d.μ
var(d::InverseGaussian) = d.μ^3 / d.λ
skewness(d::InverseGaussian) = 3.0 * sqrt(d.μ / d.λ)
kurtosis(d::InverseGaussian) = 15.0 * d.μ / d.λ
function mode(d::InverseGaussian)
    r = d.μ / d.λ
    return d.μ * (sqrt(1.0 + (1.5 * r)^2) - 1.5 * r)
end
minimum(d::InverseGaussian) = 0.0
maximum(d::InverseGaussian) = Inf

function logpdf(d::InverseGaussian, x::Real)
    x = Float64(x)
    if x <= 0.0
        return -Inf
    end
    return (log(d.λ) - (_log2π + 3.0 * log(x)) -
            d.λ * (x - d.μ)^2 / (d.μ^2 * x)) / 2.0
end
pdf(d::InverseGaussian, x::Real) = exp(logpdf(d, x))
function cdf(d::InverseGaussian, x::Real)
    x = Float64(x)
    if x <= 0.0
        return 0.0
    elseif x == Inf
        return 1.0
    end
    u = sqrt(d.λ / x)
    v = x / d.μ
    return _clamp01(_std_norm_cdf(u * (v - 1.0)) +
                    exp(2.0 * d.λ / d.μ) * _std_norm_cdf(-u * (v + 1.0)))
end
function quantile(d::InverseGaussian, q::Real)
    hi = d.μ + 20.0 * sqrt(d.μ^3 / d.λ) + 1000.0
    return _bisect_quantile(d, q, 0.0, hi)
end
rand(d::InverseGaussian) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::InverseGaussian)
    z = randn(rng)
    v = z * z
    w = d.μ * v
    x1 = d.μ + d.μ / (2.0 * d.λ) * (w - sqrt(w * (4.0 * d.λ + w)))
    p1 = d.μ / (d.μ + x1)
    return rand(rng) >= p1 ? d.μ^2 / x1 : x1
end

struct Arcsine{T<:Real} <: Distribution{Univariate, Continuous}
    a::T
    b::T
end

function Arcsine(a::Real, b::Real)
    if a >= b
        throw(ArgumentError("Arcsine: the condition a < b is not satisfied."))
    end
    x, y = promote(float(a), float(b))
    return Arcsine{typeof(x)}(x, y)
end
Arcsine(b::Real) = Arcsine(0.0, b)
Arcsine() = Arcsine(0.0, 1.0)

params(d::Arcsine) = (d.a, d.b)
location(d::Arcsine) = d.a
scale(d::Arcsine) = d.b - d.a
mean(d::Arcsine) = (d.a + d.b) / 2.0
median(d::Arcsine) = mean(d)
mode(d::Arcsine) = d.a
modes(d::Arcsine) = [d.a, d.b]
var(d::Arcsine) = (d.b - d.a)^2 / 8.0
skewness(d::Arcsine) = 0.0
kurtosis(d::Arcsine) = -1.5
entropy(d::Arcsine) = -0.24156447527049044 + log(d.b - d.a)
minimum(d::Arcsine) = d.a
maximum(d::Arcsine) = d.b

function logpdf(d::Arcsine, x::Real)
    x = Float64(x)
    if x < d.a || x > d.b
        return -Inf
    end
    return -(log(pi) + log((x - d.a) * (d.b - x)) / 2.0)
end
pdf(d::Arcsine, x::Real) = exp(logpdf(d, x))
function cdf(d::Arcsine, x::Real)
    x = Float64(x)
    if x < d.a
        return 0.0
    elseif x > d.b
        return 1.0
    end
    return (2.0 / pi) * asin(sqrt((x - d.a) / (d.b - d.a)))
end
quantile(d::Arcsine, q::Real) =
    d.a + (sin((pi / 2.0) * Float64(q))^2) * (d.b - d.a)
rand(d::Arcsine) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Arcsine) = quantile(d, _rand_open01(rng))

struct TriangularDist{T<:Real} <: Distribution{Univariate, Continuous}
    a::T
    b::T
    c::T
end

function TriangularDist(a::Real, b::Real, c::Real)
    if !(a <= c <= b)
        throw(ArgumentError("TriangularDist: the condition a <= c <= b is not satisfied."))
    end
    x, y, z = promote(float(a), float(b), float(c))
    return TriangularDist{typeof(x)}(x, y, z)
end
TriangularDist(a::Real, b::Real) = TriangularDist(a, b, (a + b) / 2.0)

params(d::TriangularDist) = (d.a, d.b, d.c)
mode(d::TriangularDist) = d.c
mean(d::TriangularDist) = (d.a + d.b + d.c) / 3.0
function median(d::TriangularDist)
    m = (d.a + d.b) / 2.0
    return d.c >= m ? d.a + sqrt((d.b - d.a) * (d.c - d.a) / 2.0) :
                      d.b - sqrt((d.b - d.a) * (d.b - d.c) / 2.0)
end
_triangular_pretvar(a::Real, b::Real, c::Real) = a*a + b*b + c*c - a*b - a*c - b*c
var(d::TriangularDist) = _triangular_pretvar(d.a, d.b, d.c) / 18.0
function skewness(d::TriangularDist)
    p = _triangular_pretvar(d.a, d.b, d.c)
    return _sqrt2 * (d.a + d.b - 2.0 * d.c) *
           (2.0 * d.a - d.b - d.c) * (d.a - 2.0 * d.b + d.c) /
           (5.0 * p^1.5)
end
kurtosis(d::TriangularDist) = -3.0 / 5.0
entropy(d::TriangularDist) = 0.5 + log((d.b - d.a) / 2.0)
minimum(d::TriangularDist) = d.a
maximum(d::TriangularDist) = d.b

function pdf(d::TriangularDist, x::Real)
    x = Float64(x)
    if x < d.a || x > d.b
        return 0.0
    elseif x < d.c
        return 2.0 * (x - d.a) / ((d.b - d.a) * (d.c - d.a))
    elseif x > d.c
        return 2.0 * (d.b - x) / ((d.b - d.a) * (d.b - d.c))
    else
        return 2.0 / (d.b - d.a)
    end
end
logpdf(d::TriangularDist, x::Real) = log(pdf(d, x))
function cdf(d::TriangularDist, x::Real)
    x = Float64(x)
    if x < d.a
        return 0.0
    elseif x >= d.b
        return 1.0
    elseif x < d.c
        return (x - d.a)^2 / ((d.b - d.a) * (d.c - d.a))
    else
        return 1.0 - (d.b - x)^2 / ((d.b - d.a) * (d.b - d.c))
    end
end
function quantile(d::TriangularDist, q::Real)
    p = Float64(q)
    cm = d.c - d.a
    bm = d.b - d.a
    return p <= cm / bm ? d.a + sqrt(bm * cm * p) :
                           d.b - sqrt(bm * (d.b - d.c) * (1.0 - p))
end
rand(d::TriangularDist) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::TriangularDist) = quantile(d, rand(rng))

struct SymTriangularDist{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    σ::T
end

function SymTriangularDist(μ::Real, σ::Real)
    if σ <= 0
        throw(ArgumentError("SymTriangularDist: the condition σ > 0 is not satisfied."))
    end
    m, s = promote(float(μ), float(σ))
    return SymTriangularDist{typeof(m)}(m, s)
end
SymTriangularDist(μ::Real) = SymTriangularDist(μ, 1.0)
SymTriangularDist() = SymTriangularDist(0.0, 1.0)

params(d::SymTriangularDist) = (d.μ, d.σ)
location(d::SymTriangularDist) = d.μ
scale(d::SymTriangularDist) = d.σ
mean(d::SymTriangularDist) = d.μ
median(d::SymTriangularDist) = d.μ
mode(d::SymTriangularDist) = d.μ
var(d::SymTriangularDist) = d.σ^2 / 6.0
skewness(d::SymTriangularDist) = 0.0
kurtosis(d::SymTriangularDist) = -3.0 / 5.0
entropy(d::SymTriangularDist) = 0.5 + log(d.σ)
minimum(d::SymTriangularDist) = d.μ - d.σ
maximum(d::SymTriangularDist) = d.μ + d.σ

_symtri_z(d::SymTriangularDist, x::Real) = min(abs(Float64(x) - d.μ) / d.σ, 1.0)
pdf(d::SymTriangularDist, x::Real) = (1.0 - _symtri_z(d, x)) / d.σ
logpdf(d::SymTriangularDist, x::Real) = log(pdf(d, x))
function cdf(d::SymTriangularDist, x::Real)
    r = (1.0 - _symtri_z(d, x))^2 / 2.0
    return Float64(x) < d.μ ? r : 1.0 - r
end
function quantile(d::SymTriangularDist, q::Real)
    p = Float64(q)
    return p < 0.5 ? d.μ + (sqrt(2.0 * p) - 1.0) * d.σ :
                     d.μ + (1.0 - sqrt(2.0 * (1.0 - p))) * d.σ
end
rand(d::SymTriangularDist) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::SymTriangularDist) = d.μ + d.σ * (rand(rng) - rand(rng))

struct Cosine{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    σ::T
end

function Cosine(μ::Real, σ::Real)
    if σ <= 0
        throw(ArgumentError("Cosine: the condition σ > 0 is not satisfied."))
    end
    m, s = promote(float(μ), float(σ))
    return Cosine{typeof(m)}(m, s)
end
Cosine(μ::Real) = Cosine(μ, 1.0)
Cosine() = Cosine(0.0, 1.0)

params(d::Cosine) = (d.μ, d.σ)
location(d::Cosine) = d.μ
scale(d::Cosine) = d.σ
mean(d::Cosine) = d.μ
median(d::Cosine) = d.μ
mode(d::Cosine) = d.μ
var(d::Cosine) = d.σ^2 * (1.0 / 3.0 - 2.0 / pi^2)
skewness(d::Cosine) = 0.0
kurtosis(d::Cosine) = 6.0 * (90.0 - pi^4) / (5.0 * (pi^2 - 6.0)^2)
minimum(d::Cosine) = d.μ - d.σ
maximum(d::Cosine) = d.μ + d.σ

function pdf(d::Cosine, x::Real)
    x = Float64(x)
    if x < minimum(d) || x > maximum(d)
        return 0.0
    end
    z = (x - d.μ) / d.σ
    return (1.0 + cospi(z)) / (2.0 * d.σ)
end
logpdf(d::Cosine, x::Real) = log(pdf(d, x))
function cdf(d::Cosine, x::Real)
    x = Float64(x)
    if x < minimum(d)
        return 0.0
    elseif x > maximum(d)
        return 1.0
    end
    z = (x - d.μ) / d.σ
    return (1.0 + z + sinpi(z) / pi) / 2.0
end
quantile(d::Cosine, q::Real) = _bisect_quantile(d, q, minimum(d), maximum(d))
rand(d::Cosine) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Cosine) = quantile(d, rand(rng))

struct Semicircle{T<:Real} <: Distribution{Univariate, Continuous}
    r::T
end

function Semicircle(r::Real)
    if r <= 0
        throw(ArgumentError("Semicircle: the condition r > 0 is not satisfied."))
    end
    rr = float(r)
    return Semicircle{typeof(rr)}(rr)
end

params(d::Semicircle) = (d.r,)
mean(d::Semicircle) = 0.0
median(d::Semicircle) = 0.0
mode(d::Semicircle) = 0.0
var(d::Semicircle) = d.r^2 / 4.0
skewness(d::Semicircle) = 0.0
entropy(d::Semicircle) = log(pi * d.r) - 0.5
minimum(d::Semicircle) = -d.r
maximum(d::Semicircle) = d.r

function logpdf(d::Semicircle, x::Real)
    x = Float64(x)
    if x < -d.r || x > d.r
        return -Inf
    end
    return log(2.0 / pi) - 2.0 * log(d.r) + log(d.r^2 - x^2) / 2.0
end
pdf(d::Semicircle, x::Real) = exp(logpdf(d, x))
function cdf(d::Semicircle, x::Real)
    x = Float64(x)
    if x < -d.r
        return 0.0
    elseif x > d.r
        return 1.0
    end
    u = x / d.r
    return (u * sqrt(1.0 - u^2) + asin(u)) / pi + 0.5
end
quantile(d::Semicircle, q::Real) = _bisect_quantile(d, q, -d.r, d.r)
rand(d::Semicircle) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::Semicircle)
    θ = rand(rng)
    r = d.r * sqrt(rand(rng))
    return cospi(θ) * r
end

struct Kumaraswamy{T<:Real} <: Distribution{Univariate, Continuous}
    a::T
    b::T
end

function Kumaraswamy(a::Real, b::Real)
    if a <= 0 || b <= 0
        throw(ArgumentError("Kumaraswamy: the conditions a > 0 and b > 0 are not satisfied."))
    end
    x, y = promote(float(a), float(b))
    return Kumaraswamy{typeof(x)}(x, y)
end
Kumaraswamy() = Kumaraswamy(1.0, 1.0)

params(d::Kumaraswamy) = (d.a, d.b)
minimum(d::Kumaraswamy) = 0.0
maximum(d::Kumaraswamy) = 1.0

function logpdf(d::Kumaraswamy, x::Real)
    x = Float64(x)
    if x < 0.0 || x > 1.0
        return -Inf
    end
    return log(d.a) + log(d.b) + (d.a - 1.0) * log(x) +
           (d.b - 1.0) * log(1.0 - x^d.a)
end
pdf(d::Kumaraswamy, x::Real) = exp(logpdf(d, x))
cdf(d::Kumaraswamy, x::Real) =
    Float64(x) < 0.0 ? 0.0 :
    (Float64(x) > 1.0 ? 1.0 : 1.0 - (1.0 - Float64(x)^d.a)^d.b)
quantile(d::Kumaraswamy, q::Real) =
    (1.0 - (1.0 - Float64(q))^(1.0 / d.b))^(1.0 / d.a)
function entropy(d::Kumaraswamy)
    h = _digamma_approx(d.b + 1.0) + _eulergamma
    return (1.0 - 1.0 / d.b) + (1.0 - 1.0 / d.a) * h - log(d.a) - log(d.b)
end
_kumomentaswamy(a, b, n) = b * beta(1.0 + n / a, b)
mean(d::Kumaraswamy) = _kumomentaswamy(d.a, d.b, 1.0)
function var(d::Kumaraswamy)
    m1 = _kumomentaswamy(d.a, d.b, 1.0)
    m2 = _kumomentaswamy(d.a, d.b, 2.0)
    return m2 - m1^2
end
function skewness(d::Kumaraswamy)
    μ = mean(d)
    σ2 = var(d)
    m2 = _kumomentaswamy(d.a, d.b, 2.0)
    m3 = _kumomentaswamy(d.a, d.b, 3.0)
    return (2.0 * m3 - μ * (3.0 * m2 - μ^2)) / (σ2 * sqrt(σ2))
end
function kurtosis(d::Kumaraswamy)
    μ = mean(d)
    m2 = _kumomentaswamy(d.a, d.b, 2.0)
    m3 = _kumomentaswamy(d.a, d.b, 3.0)
    m4 = _kumomentaswamy(d.a, d.b, 4.0)
    return (m4 + μ * (-4.0 * m3 + μ * (6.0 * m2 - 3.0 * μ^2))) / var(d)^2 - 3.0
end
median(d::Kumaraswamy) = (1.0 - 2.0^(-1.0 / d.b))^(1.0 / d.a)
function mode(d::Kumaraswamy)
    m = ((d.a - 1.0) / (d.a * d.b - 1.0))^(1.0 / d.a)
    return d.a >= 1.0 && d.b >= 1.0 && !(d.a == 1.0 && d.b == 1.0) ? m : NaN
end
rand(d::Kumaraswamy) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Kumaraswamy) = quantile(d, _rand_open01(rng))

# ── Beta ────────────────────────────────────────────────────────────────────

struct Beta{T<:Real} <: Distribution{Univariate, Continuous}
    α::T
    β::T
end

function Beta(α::Real, β::Real)
    if α <= 0 || β <= 0
        throw(ArgumentError("Beta: the conditions α > 0 and β > 0 are not satisfied."))
    end
    a, b = promote(float(α), float(β))
    return Beta{typeof(a)}(a, b)
end
Beta(α::Real) = Beta(α, α)
Beta() = Beta(1.0, 1.0)

params(d::Beta) = (d.α, d.β)
mean(d::Beta) = d.α / (d.α + d.β)
var(d::Beta) = (d.α * d.β) / ((d.α + d.β)^2 * (d.α + d.β + 1.0))
function mode(d::Beta)
    if d.α > 1.0 && d.β > 1.0
        return (d.α - 1.0) / (d.α + d.β - 2.0)
    end
    error("Beta: mode is defined only for α > 1 and β > 1")
end
modes(d::Beta) = [mode(d)]
function skewness(d::Beta)
    if d.α == d.β
        return 0.0
    end
    s = d.α + d.β
    return (2.0 * (d.β - d.α) * sqrt(s + 1.0)) /
           ((s + 2.0) * sqrt(d.α * d.β))
end
function kurtosis(d::Beta)
    s = d.α + d.β
    p = d.α * d.β
    return 6.0 * ((d.α - d.β)^2 * (s + 1.0) - p * (s + 2.0)) /
           (p * (s + 2.0) * (s + 3.0))
end
minimum(d::Beta) = 0.0
maximum(d::Beta) = 1.0

function pdf(d::Beta, x::Real)
    x = Float64(x)
    if x < 0.0 || x > 1.0
        return 0.0
    end
    return exp((d.α - 1.0) * log(x) + (d.β - 1.0) * log(1.0 - x) - lbeta(d.α, d.β))
end
cdf(d::Beta, x::Real) = Float64(x) <= 0.0 ? 0.0 : (Float64(x) >= 1.0 ? 1.0 : beta_inc(d.α, d.β, Float64(x)))
quantile(d::Beta, q::Real) = _bisect_quantile(d, q, 0.0, 1.0)
function rand(d::Beta)
    x = _rand_gamma_shape(Float64(d.α))
    y = _rand_gamma_shape(Float64(d.β))
    return x / (x + y)
end
function _rand_scalar(rng, d::Beta)
    x = _rand_gamma_shape(rng, Float64(d.α))
    y = _rand_gamma_shape(rng, Float64(d.β))
    return x / (x + y)
end

# ── Laplace ─────────────────────────────────────────────────────────────────

struct Laplace{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    θ::T
end

function Laplace(μ::Real, θ::Real)
    if θ <= 0
        throw(ArgumentError("Laplace: the condition θ > 0 is not satisfied."))
    end
    m, t = promote(float(μ), float(θ))
    return Laplace{typeof(m)}(m, t)
end
Laplace(μ::Real) = Laplace(μ, 1.0)
Laplace() = Laplace(0.0, 1.0)

params(d::Laplace) = (d.μ, d.θ)
location(d::Laplace) = d.μ
scale(d::Laplace) = d.θ
mean(d::Laplace) = d.μ
median(d::Laplace) = d.μ
mode(d::Laplace) = d.μ
var(d::Laplace) = 2.0 * d.θ^2
std(d::Laplace) = _sqrt2 * d.θ
skewness(d::Laplace) = 0.0
kurtosis(d::Laplace) = 3.0
entropy(d::Laplace) = log(2.0 * d.θ) + 1.0
minimum(d::Laplace) = -Inf
maximum(d::Laplace) = Inf

pdf(d::Laplace, x::Real) = exp(-abs((x - d.μ) / d.θ)) / (2.0 * d.θ)
logpdf(d::Laplace, x::Real) = -(abs((x - d.μ) / d.θ) + log(2.0 * d.θ))
function cdf(d::Laplace, x::Real)
    z = (Float64(x) - d.μ) / d.θ
    return z < 0.0 ? exp(z) / 2.0 : 1.0 - exp(-z) / 2.0
end
function quantile(d::Laplace, q::Real)
    p = Float64(q)
    return p < 0.5 ? d.μ + d.θ * log(2.0 * p) : d.μ - d.θ * log(2.0 * (1.0 - p))
end
cquantile(d::Laplace, p::Real) =
    Float64(p) > 0.5 ? d.μ + d.θ * log(2.0 * (1.0 - Float64(p))) :
                       d.μ - d.θ * log(2.0 * Float64(p))
mgf(d::Laplace, t::Real) = exp(Float64(t) * d.μ) / (1.0 - (d.θ * Float64(t))^2)
cf(d::Laplace, t::Real) = cis(Float64(t) * d.μ) / (1.0 + (d.θ * Float64(t))^2)
rand(d::Laplace) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::Laplace)
    e = _randexp_unit(rng)
    s = rand(rng) < 0.5 ? -1.0 : 1.0
    return d.μ + d.θ * s * e
end

# ── Logistic ────────────────────────────────────────────────────────────────

struct Logistic{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    θ::T
end

function Logistic(μ::Real, θ::Real)
    if θ <= 0
        throw(ArgumentError("Logistic: the condition θ > 0 is not satisfied."))
    end
    m, t = promote(float(μ), float(θ))
    return Logistic{typeof(m)}(m, t)
end
Logistic(μ::Real) = Logistic(μ, 1.0)
Logistic() = Logistic(0.0, 1.0)

params(d::Logistic) = (d.μ, d.θ)
location(d::Logistic) = d.μ
scale(d::Logistic) = d.θ
mean(d::Logistic) = d.μ
median(d::Logistic) = d.μ
mode(d::Logistic) = d.μ
var(d::Logistic) = (pi * d.θ)^2 / 3.0
std(d::Logistic) = pi * d.θ / sqrt(3.0)
skewness(d::Logistic) = 0.0
kurtosis(d::Logistic) = 6.0 / 5.0
entropy(d::Logistic) = log(d.θ) + 2.0
minimum(d::Logistic) = -Inf
maximum(d::Logistic) = Inf

function pdf(d::Logistic, x::Real)
    z = (Float64(x) - d.μ) / d.θ
    e = exp(-abs(z))
    return e / (d.θ * (1.0 + e)^2)
end
function logpdf(d::Logistic, x::Real)
    u = -abs((Float64(x) - d.μ) / d.θ)
    return u - 2.0 * log(1.0 + exp(u)) - log(d.θ)
end
cdf(d::Logistic, x::Real) = 1.0 / (1.0 + exp(-((Float64(x) - d.μ) / d.θ)))
quantile(d::Logistic, q::Real) = d.μ + d.θ * log(Float64(q) / (1.0 - Float64(q)))
cquantile(d::Logistic, p::Real) = d.μ - d.θ * log(Float64(p) / (1.0 - Float64(p)))
mgf(d::Logistic, t::Real) = exp(Float64(t) * d.μ) / sinc(d.θ * Float64(t))
function cf(d::Logistic, t::Real)
    a = pi * Float64(t) * d.θ
    return a == 0.0 ? Complex{Float64}(1.0, 0.0) : cis(Float64(t) * d.μ) * (a / sinh(a))
end
rand(d::Logistic) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Logistic) = quantile(d, _rand_open01(rng))

# ── Rayleigh ────────────────────────────────────────────────────────────────

struct Rayleigh{T<:Real} <: Distribution{Univariate, Continuous}
    σ::T
end

function Rayleigh(σ::Real)
    if σ <= 0
        throw(ArgumentError("Rayleigh: the condition σ > 0 is not satisfied."))
    end
    s = float(σ)
    return Rayleigh{typeof(s)}(s)
end
Rayleigh() = Rayleigh(1.0)

params(d::Rayleigh) = (d.σ,)
scale(d::Rayleigh) = d.σ
mean(d::Rayleigh) = (_sqrtπ / _sqrt2) * d.σ
median(d::Rayleigh) = d.σ * sqrt(2.0 * _log2)
mode(d::Rayleigh) = d.σ
var(d::Rayleigh) = (2.0 - pi / 2.0) * d.σ^2
std(d::Rayleigh) = sqrt(2.0 - pi / 2.0) * d.σ
skewness(d::Rayleigh) = 2.0 * _sqrtπ * (pi - 3.0) / (4.0 - pi)^1.5
kurtosis(d::Rayleigh) = -(6.0 * pi^2 - 24.0 * pi + 16.0) / (4.0 - pi)^2
entropy(d::Rayleigh) = 1.0 - _log2 / 2.0 + _eulergamma / 2.0 + log(d.σ)
minimum(d::Rayleigh) = 0.0
maximum(d::Rayleigh) = Inf

function logpdf(d::Rayleigh, x::Real)
    x = Float64(x)
    if x <= 0.0
        return -Inf
    end
    σ2 = d.σ^2
    return log(x / σ2) - x^2 / (2.0 * σ2)
end
pdf(d::Rayleigh, x::Real) = exp(logpdf(d, x))
cdf(d::Rayleigh, x::Real) =
    Float64(x) <= 0.0 ? 0.0 : 1.0 - exp(-(Float64(x)^2) / (2.0 * d.σ^2))
ccdf(d::Rayleigh, x::Real) =
    Float64(x) <= 0.0 ? 1.0 : exp(-(Float64(x)^2) / (2.0 * d.σ^2))
quantile(d::Rayleigh, q::Real) = d.σ * sqrt(-2.0 * log(1.0 - Float64(q)))
rand(d::Rayleigh) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Rayleigh) = d.σ * sqrt(2.0 * _randexp_unit(rng))

# ── Pareto ──────────────────────────────────────────────────────────────────

struct Pareto{T<:Real} <: Distribution{Univariate, Continuous}
    α::T
    θ::T
end

function Pareto(α::Real, θ::Real)
    if α <= 0 || θ <= 0
        throw(ArgumentError("Pareto: the conditions α > 0 and θ > 0 are not satisfied."))
    end
    a, t = promote(float(α), float(θ))
    return Pareto{typeof(a)}(a, t)
end
Pareto(α::Real) = Pareto(α, 1.0)
Pareto() = Pareto(1.0, 1.0)

params(d::Pareto) = (d.α, d.θ)
shape(d::Pareto) = d.α
scale(d::Pareto) = d.θ
function mean(d::Pareto)
    return d.α > 1.0 ? d.α * d.θ / (d.α - 1.0) : Inf
end
median(d::Pareto) = d.θ * 2.0^(1.0 / d.α)
mode(d::Pareto) = d.θ
function var(d::Pareto)
    return d.α > 2.0 ? (d.θ^2 * d.α) / ((d.α - 1.0)^2 * (d.α - 2.0)) : Inf
end
function skewness(d::Pareto)
    return d.α > 3.0 ? ((2.0 * (1.0 + d.α)) / (d.α - 3.0)) * sqrt((d.α - 2.0) / d.α) : NaN
end
function kurtosis(d::Pareto)
    α = d.α
    return α > 4.0 ? (6.0 * (α^3 + α^2 - 6.0 * α - 2.0)) / (α * (α - 3.0) * (α - 4.0)) : NaN
end
entropy(d::Pareto) = log(d.θ / d.α) + 1.0 / d.α + 1.0
minimum(d::Pareto) = d.θ
maximum(d::Pareto) = Inf

function logpdf(d::Pareto, x::Real)
    x = Float64(x)
    if x < d.θ
        return -Inf
    end
    return log(d.α) + d.α * log(d.θ) - (d.α + 1.0) * log(x)
end
pdf(d::Pareto, x::Real) = exp(logpdf(d, x))
cdf(d::Pareto, x::Real) = Float64(x) < d.θ ? 0.0 : 1.0 - (d.θ / Float64(x))^d.α
ccdf(d::Pareto, x::Real) = (d.θ / max(Float64(x), d.θ))^d.α
quantile(d::Pareto, q::Real) = d.θ / (1.0 - Float64(q))^(1.0 / d.α)
cquantile(d::Pareto, p::Real) = d.θ / Float64(p)^(1.0 / d.α)
rand(d::Pareto) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Pareto) = d.θ * exp(_randexp_unit(rng) / d.α)

# ── Gumbel ──────────────────────────────────────────────────────────────────

struct Gumbel{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    θ::T
end

function Gumbel(μ::Real, θ::Real)
    if θ <= 0
        throw(ArgumentError("Gumbel: the condition θ > 0 is not satisfied."))
    end
    m, t = promote(float(μ), float(θ))
    return Gumbel{typeof(m)}(m, t)
end
Gumbel(μ::Real) = Gumbel(μ, 1.0)
Gumbel() = Gumbel(0.0, 1.0)

params(d::Gumbel) = (d.μ, d.θ)
location(d::Gumbel) = d.μ
scale(d::Gumbel) = d.θ
mean(d::Gumbel) = d.μ + d.θ * _eulergamma
median(d::Gumbel) = d.μ - d.θ * log(_log2)
mode(d::Gumbel) = d.μ
var(d::Gumbel) = pi^2 * d.θ^2 / 6.0
skewness(d::Gumbel) = _gumbel_skewness
kurtosis(d::Gumbel) = 12.0 / 5.0
entropy(d::Gumbel) = log(d.θ) + 1.0 + _eulergamma
minimum(d::Gumbel) = -Inf
maximum(d::Gumbel) = Inf

function logpdf(d::Gumbel, x::Real)
    z = (Float64(x) - d.μ) / d.θ
    return -(z + exp(-z) + log(d.θ))
end
pdf(d::Gumbel, x::Real) = exp(logpdf(d, x))
cdf(d::Gumbel, x::Real) = exp(-exp(-((Float64(x) - d.μ) / d.θ)))
quantile(d::Gumbel, q::Real) = d.μ - d.θ * log(-log(Float64(q)))
rand(d::Gumbel) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Gumbel) = d.μ - d.θ * log(_randexp_unit(rng))

# ── Frechet ─────────────────────────────────────────────────────────────────

struct Frechet{T<:Real} <: Distribution{Univariate, Continuous}
    α::T
    θ::T
end

function Frechet(α::Real, θ::Real)
    if α <= 0 || θ <= 0
        throw(ArgumentError("Frechet: the conditions α > 0 and θ > 0 are not satisfied."))
    end
    a, t = promote(float(α), float(θ))
    return Frechet{typeof(a)}(a, t)
end
Frechet(α::Real) = Frechet(α, 1.0)
Frechet() = Frechet(1.0, 1.0)

params(d::Frechet) = (d.α, d.θ)
shape(d::Frechet) = d.α
scale(d::Frechet) = d.θ
function mean(d::Frechet)
    return d.α > 1.0 ? d.θ * gamma(1.0 - 1.0 / d.α) : Inf
end
median(d::Frechet) = d.θ * _log2^(-1.0 / d.α)
mode(d::Frechet) = d.θ * (d.α / (d.α + 1.0))^(1.0 / d.α)
function var(d::Frechet)
    if d.α <= 2.0
        return Inf
    end
    iα = 1.0 / d.α
    return d.θ^2 * (gamma(1.0 - 2.0 * iα) - gamma(1.0 - iα)^2)
end
function skewness(d::Frechet)
    if d.α <= 3.0
        return Inf
    end
    iα = 1.0 / d.α
    g1 = gamma(1.0 - iα)
    g2 = gamma(1.0 - 2.0 * iα)
    g3 = gamma(1.0 - 3.0 * iα)
    return (g3 - 3.0 * g2 * g1 + 2.0 * g1^3) / ((g2 - g1^2)^1.5)
end
function kurtosis(d::Frechet)
    if d.α <= 3.0
        return Inf
    end
    iα = 1.0 / d.α
    g1 = gamma(1.0 - iα)
    g2 = gamma(1.0 - 2.0 * iα)
    g3 = gamma(1.0 - 3.0 * iα)
    g4 = gamma(1.0 - 4.0 * iα)
    return (g4 - 4.0 * g3 * g1 + 3.0 * g2^2) / ((g2 - g1^2)^2) - 6.0
end
entropy(d::Frechet) = 1.0 + _eulergamma / d.α + _eulergamma + log(d.θ / d.α)
minimum(d::Frechet) = 0.0
maximum(d::Frechet) = Inf

function logpdf(d::Frechet, x::Real)
    x = Float64(x)
    if x <= 0.0
        return -Inf
    end
    z = d.θ / x
    return log(d.α / d.θ) + (1.0 + d.α) * log(z) - z^d.α
end
pdf(d::Frechet, x::Real) = exp(logpdf(d, x))
cdf(d::Frechet, x::Real) =
    Float64(x) <= 0.0 ? 0.0 : exp(-((d.θ / Float64(x))^d.α))
quantile(d::Frechet, q::Real) = d.θ * (-log(Float64(q)))^(-1.0 / d.α)
rand(d::Frechet) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Frechet) = d.θ * _randexp_unit(rng)^(-1.0 / d.α)

# ── Levy ────────────────────────────────────────────────────────────────────

struct Levy{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    σ::T
end

function Levy(μ::Real, σ::Real)
    if σ <= 0
        throw(ArgumentError("Levy: the condition σ > 0 is not satisfied."))
    end
    m, s = promote(float(μ), float(σ))
    return Levy{typeof(m)}(m, s)
end
Levy(μ::Real) = Levy(μ, 1.0)
Levy() = Levy(0.0, 1.0)

params(d::Levy) = (d.μ, d.σ)
location(d::Levy) = d.μ
mean(d::Levy) = Inf
var(d::Levy) = Inf
skewness(d::Levy) = NaN
kurtosis(d::Levy) = NaN
mode(d::Levy) = d.μ + d.σ / 3.0
entropy(d::Levy) = (1.0 + 3.0 * _eulergamma + log(16.0 * d.σ^2 * pi)) / 2.0
median(d::Levy) = quantile(d, 0.5)
minimum(d::Levy) = d.μ
maximum(d::Levy) = Inf

function logpdf(d::Levy, x::Real)
    x = Float64(x)
    if x <= d.μ
        return -Inf
    end
    z = x - d.μ
    return (log(d.σ) - _log2π - d.σ / z - 3.0 * log(z)) / 2.0
end
pdf(d::Levy, x::Real) = exp(logpdf(d, x))
cdf(d::Levy, x::Real) =
    Float64(x) <= d.μ ? 0.0 : erfc(sqrt(d.σ / (2.0 * (Float64(x) - d.μ))))
ccdf(d::Levy, x::Real) =
    Float64(x) <= d.μ ? 1.0 : erf(sqrt(d.σ / (2.0 * (Float64(x) - d.μ))))
function quantile(d::Levy, q::Real)
    p = Float64(q)
    z = _norminvcdf(p / 2.0)
    return d.μ + d.σ / (z * z)
end
function cquantile(d::Levy, p::Real)
    z = _norminvcdf((1.0 + Float64(p)) / 2.0)
    return d.μ + d.σ / (z * z)
end
rand(d::Levy) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::Levy)
    z = randn(rng)
    return d.μ + d.σ / (z * z)
end

# ── Cauchy ──────────────────────────────────────────────────────────────────

struct Cauchy{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    σ::T
end

function Cauchy(μ::Real, σ::Real)
    if σ <= 0
        throw(ArgumentError("Cauchy: the condition σ > 0 is not satisfied."))
    end
    m, s = promote(float(μ), float(σ))
    return Cauchy{typeof(m)}(m, s)
end
Cauchy(μ::Real) = Cauchy(μ, 1.0)
Cauchy() = Cauchy(0.0, 1.0)

params(d::Cauchy) = (d.μ, d.σ)
location(d::Cauchy) = d.μ
scale(d::Cauchy) = d.σ
mean(d::Cauchy) = NaN
median(d::Cauchy) = d.μ
mode(d::Cauchy) = d.μ
var(d::Cauchy) = NaN
skewness(d::Cauchy) = NaN
kurtosis(d::Cauchy) = NaN
entropy(d::Cauchy) = log(4.0 * pi * d.σ)
minimum(d::Cauchy) = -Inf
maximum(d::Cauchy) = Inf

pdf(d::Cauchy, x::Real) = 1.0 / (pi * d.σ * (1.0 + ((x - d.μ) / d.σ)^2))
cdf(d::Cauchy, x::Real) = 0.5 + atan((x - d.μ) / d.σ) / pi
quantile(d::Cauchy, q::Real) = d.μ + d.σ * tan(pi * (Float64(q) - 0.5))
cquantile(d::Cauchy, p::Real) = d.μ + d.σ * tan(pi * (0.5 - Float64(p)))
cf(d::Cauchy, t::Real) = exp(-d.σ * abs(Float64(t))) * cis(d.μ * Float64(t))
rand(d::Cauchy) = d.μ + d.σ * tan(pi * (rand() - 0.5))
_rand_scalar(rng, d::Cauchy) = d.μ + d.σ * tan(pi * (rand(rng) - 0.5))

# ── LogNormal ───────────────────────────────────────────────────────────────

struct LogNormal{T<:Real} <: Distribution{Univariate, Continuous}
    μ::T
    σ::T
end

function LogNormal(μ::Real, σ::Real)
    if σ < 0
        throw(ArgumentError("LogNormal: the condition σ >= 0 is not satisfied."))
    end
    m, s = promote(float(μ), float(σ))
    return LogNormal{typeof(m)}(m, s)
end
LogNormal(μ::Real) = LogNormal(μ, 1.0)
LogNormal() = LogNormal(0.0, 1.0)

params(d::LogNormal) = (d.μ, d.σ)
meanlogx(d::LogNormal) = d.μ
varlogx(d::LogNormal) = d.σ^2
mean(d::LogNormal) = exp(d.μ + d.σ^2 / 2.0)
median(d::LogNormal) = exp(d.μ)
mode(d::LogNormal) = exp(d.μ - d.σ^2)
var(d::LogNormal) = (exp(d.σ^2) - 1.0) * exp(2.0 * d.μ + d.σ^2)
function skewness(d::LogNormal)
    e = exp(d.σ^2)
    return (e + 2.0) * sqrt(e - 1.0)
end
function kurtosis(d::LogNormal)
    e = exp(d.σ^2)
    e2 = e * e
    e3 = e2 * e
    e4 = e3 * e
    return e4 + 2.0 * e3 + 3.0 * e2 - 6.0
end
minimum(d::LogNormal) = 0.0
maximum(d::LogNormal) = Inf

function pdf(d::LogNormal, x::Real)
    x = Float64(x)
    if x <= 0.0
        return 0.0
    end
    return exp(-(log(x) - d.μ)^2 / (2.0 * d.σ^2)) / (x * d.σ * _sqrt2π)
end
cdf(d::LogNormal, x::Real) = Float64(x) <= 0.0 ? 0.0 : 0.5 * erfc(-(log(Float64(x)) - d.μ) / (d.σ * _sqrt2))
quantile(d::LogNormal, q::Real) = exp(d.μ + d.σ * _norminvcdf(Float64(q)))
function kldivergence(p::LogNormal, q::LogNormal)
    return kldivergence(Normal(p.μ, p.σ), Normal(q.μ, q.σ))
end
rand(d::LogNormal) = exp(d.μ + d.σ * randn())
_rand_scalar(rng, d::LogNormal) = exp(d.μ + d.σ * randn(rng))

# ── Weibull (shape α, scale θ) ──────────────────────────────────────────────

struct Weibull{T<:Real} <: Distribution{Univariate, Continuous}
    α::T
    θ::T
end

function Weibull(α::Real, θ::Real)
    if α <= 0 || θ <= 0
        throw(ArgumentError("Weibull: the conditions α > 0 and θ > 0 are not satisfied."))
    end
    a, t = promote(float(α), float(θ))
    return Weibull{typeof(a)}(a, t)
end
Weibull(α::Real) = Weibull(α, 1.0)
Weibull() = Weibull(1.0, 1.0)

params(d::Weibull) = (d.α, d.θ)
shape(d::Weibull) = d.α
scale(d::Weibull) = d.θ
mean(d::Weibull) = d.θ * gamma(1.0 + 1.0 / d.α)
median(d::Weibull) = d.θ * log(2.0)^(1.0 / d.α)
function mode(d::Weibull)
    if d.α > 1.0
        return d.θ * ((d.α - 1.0) / d.α)^(1.0 / d.α)
    end
    return 0.0
end
var(d::Weibull) = d.θ^2 * (gamma(1.0 + 2.0 / d.α) - gamma(1.0 + 1.0 / d.α)^2)
function skewness(d::Weibull)
    μ = mean(d)
    σ2 = var(d)
    σ = sqrt(σ2)
    r = μ / σ
    return gamma(1.0 + 3.0 / d.α) * (d.θ / σ)^3 - 3.0 * r - r^3
end
function kurtosis(d::Weibull)
    μ = mean(d)
    σ = sqrt(var(d))
    γ = skewness(d)
    r = μ / σ
    r2 = r^2
    return (d.θ / σ)^4 * gamma(1.0 + 4.0 / d.α) -
           4.0 * γ * r - 6.0 * r2 - r2^2 - 3.0
end
minimum(d::Weibull) = 0.0
maximum(d::Weibull) = Inf

function pdf(d::Weibull, x::Real)
    x = Float64(x)
    if x < 0.0
        return 0.0
    end
    z = x / d.θ
    return (d.α / d.θ) * z^(d.α - 1.0) * exp(-(z^d.α))
end
cdf(d::Weibull, x::Real) = Float64(x) < 0.0 ? 0.0 : 1.0 - exp(-(Float64(x) / d.θ)^d.α)
quantile(d::Weibull, q::Real) = d.θ * (-log(1.0 - Float64(q)))^(1.0 / d.α)
rand(d::Weibull) = d.θ * (-log(1.0 - rand()))^(1.0 / d.α)
_rand_scalar(rng, d::Weibull) = d.θ * (-log(1.0 - rand(rng)))^(1.0 / d.α)
