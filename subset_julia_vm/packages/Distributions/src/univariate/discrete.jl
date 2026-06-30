# Univariate discrete distributions (Issue #7178, Phase 3).
#
# Each distribution implements the common API; for discrete distributions `pdf`
# is the probability mass function. Support values are integers. Sampling
# supports both global and explicit RNGs through the shared API in
# Distributions.jl.
#
# NOTE: constructors bind the promoted parameter to a local before forming the
# parametric type (`v = float(p); Bernoulli{typeof(v)}(v)`) because a nested
# function call inside the `{...}` type-parameter braces fails to compile in the
# VM (Issue #7240).

# log binomial coefficient log C(n, k) via loggamma (numerically stable).
_logbinom(n::Real, k::Real) =
    loggamma(n + 1.0) - loggamma(k + 1.0) - loggamma(n - k + 1.0)

function _xlogratio(x::Real, y::Real)
    if x == 0.0
        return 0.0
    elseif y == 0.0
        return Inf
    else
        return x * log(x / y)
    end
end

# Scan-based quantile for an integer-support distribution: smallest integer k in
# [lo, hi] with cdf(d, k) >= q.
function _discrete_quantile(d, q::Real, lo::Int, hi::Int)
    q = Float64(q)
    if q <= 0.0
        return lo
    end
    k = lo
    while k < hi
        if cdf(d, k) >= q
            return k
        end
        k += 1
    end
    return hi
end

function _is_integer_value(x::Real)
    y = Float64(x)
    return y != Inf && y != -Inf && y == floor(y)
end

# ── Bernoulli ───────────────────────────────────────────────────────────────

struct Bernoulli{T<:Real} <: Distribution{Univariate, Discrete}
    p::T
end

function Bernoulli(p::Real)
    if p < 0 || p > 1
        throw(ArgumentError("Bernoulli: the condition 0 <= p <= 1 is not satisfied."))
    end
    v = float(p)
    return Bernoulli{typeof(v)}(v)
end
Bernoulli() = Bernoulli(0.5)

params(d::Bernoulli) = (d.p,)
succprob(d::Bernoulli) = d.p
failprob(d::Bernoulli) = 1.0 - d.p
mean(d::Bernoulli) = d.p
var(d::Bernoulli) = d.p * (1.0 - d.p)
mode(d::Bernoulli) = d.p > 0.5 ? 1 : 0
function modes(d::Bernoulli)
    if d.p < 0.5
        return [0]
    elseif d.p > 0.5
        return [1]
    else
        return [0, 1]
    end
end
skewness(d::Bernoulli) = ((1.0 - d.p) - d.p) / sqrt((1.0 - d.p) * d.p)
kurtosis(d::Bernoulli) = 1.0 / var(d) - 6.0
function entropy(d::Bernoulli)
    if d.p == 0.0 || d.p == 1.0
        return 0.0
    end
    return -d.p * log(d.p) - (1.0 - d.p) * log(1.0 - d.p)
end
minimum(d::Bernoulli) = 0
maximum(d::Bernoulli) = 1

function pdf(d::Bernoulli, k::Real)
    if k == 0
        return 1.0 - d.p
    elseif k == 1
        return d.p
    else
        return 0.0
    end
end
function cdf(d::Bernoulli, k::Real)
    if k < 0
        return 0.0
    elseif k < 1
        return 1.0 - d.p
    else
        return 1.0
    end
end
quantile(d::Bernoulli, q::Real) = Float64(q) <= 1.0 - d.p ? 0 : 1
cquantile(d::Bernoulli, p::Real) = Float64(p) >= d.p ? 0 : 1
mgf(d::Bernoulli, t::Real) = (1.0 - d.p) + d.p * exp(Float64(t))
cf(d::Bernoulli, t::Real) = (1.0 - d.p) + d.p * cis(Float64(t))
function kldivergence(p::Bernoulli, q::Bernoulli)
    return _xlogratio(1.0 - p.p, 1.0 - q.p) + _xlogratio(p.p, q.p)
end
rand(d::Bernoulli) = rand() < d.p ? 1 : 0
_rand_scalar(rng, d::Bernoulli) = rand(rng) < d.p ? 1 : 0

# ── Binomial ────────────────────────────────────────────────────────────────

struct Binomial{T<:Real} <: Distribution{Univariate, Discrete}
    n::Int
    p::T
end

function Binomial(n::Integer, p::Real)
    if n < 0
        throw(ArgumentError("Binomial: the condition n >= 0 is not satisfied."))
    end
    if p < 0 || p > 1
        throw(ArgumentError("Binomial: the condition 0 <= p <= 1 is not satisfied."))
    end
    v = float(p)
    return Binomial{typeof(v)}(Int(n), v)
end
Binomial(n::Integer) = Binomial(n, 0.5)

params(d::Binomial) = (d.n, d.p)
ntrials(d::Binomial) = d.n
succprob(d::Binomial) = d.p
failprob(d::Binomial) = 1.0 - d.p
mean(d::Binomial) = d.n * d.p
var(d::Binomial) = d.n * d.p * (1.0 - d.p)
mode(d::Binomial) = Int(floor((d.n + 1) * d.p))
modes(d::Binomial) = [mode(d)]
function skewness(d::Binomial)
    p0 = 1.0 - d.p
    return (p0 - d.p) / sqrt(d.n * p0 * d.p)
end
function kurtosis(d::Binomial)
    u = d.p * (1.0 - d.p)
    return (1.0 - 6.0 * u) / (d.n * u)
end
minimum(d::Binomial) = 0
maximum(d::Binomial) = d.n

function pdf(d::Binomial, k::Real)
    k = Float64(k)
    if k < 0.0 || k > d.n || k != floor(k)
        return 0.0
    end
    if d.p == 0.0
        return k == 0.0 ? 1.0 : 0.0
    elseif d.p == 1.0
        return k == d.n ? 1.0 : 0.0
    end
    return exp(_logbinom(d.n, k) + k * log(d.p) + (d.n - k) * log(1.0 - d.p))
end
function cdf(d::Binomial, k::Real)
    kk = floor(Float64(k))
    if kk < 0.0
        return 0.0
    elseif kk >= d.n
        return 1.0
    end
    # P(X <= k) = I_{1-p}(n-k, k+1)
    return beta_inc(d.n - kk, kk + 1.0, 1.0 - d.p)
end
quantile(d::Binomial, q::Real) = _discrete_quantile(d, q, 0, d.n)
mgf(d::Binomial, t::Real) = ((1.0 - d.p) + d.p * exp(Float64(t)))^d.n
cf(d::Binomial, t::Real) = ((1.0 - d.p) + d.p * cis(Float64(t)))^d.n
function kldivergence(p::Binomial, q::Binomial)
    if p.n != q.n
        error("kldivergence: Binomial distributions must have the same n")
    end
    return p.n * kldivergence(Bernoulli(p.p), Bernoulli(q.p))
end
function rand(d::Binomial)
    c = 0
    for _ in 1:d.n
        if rand() < d.p
            c += 1
        end
    end
    return c
end
function _rand_scalar(rng, d::Binomial)
    c = 0
    for _ in 1:d.n
        if rand(rng) < d.p
            c += 1
        end
    end
    return c
end

# ── Poisson ─────────────────────────────────────────────────────────────────

struct Poisson{T<:Real} <: Distribution{Univariate, Discrete}
    λ::T
end

function Poisson(λ::Real)
    if λ < 0
        throw(ArgumentError("Poisson: the condition λ >= 0 is not satisfied."))
    end
    v = float(λ)
    return Poisson{typeof(v)}(v)
end
Poisson() = Poisson(1.0)

params(d::Poisson) = (d.λ,)
rate(d::Poisson) = d.λ
mean(d::Poisson) = d.λ
var(d::Poisson) = d.λ
mode(d::Poisson) = Int(floor(d.λ))
modes(d::Poisson) = d.λ == floor(d.λ) ? [Int(d.λ) - 1, Int(d.λ)] : [Int(floor(d.λ))]
minimum(d::Poisson) = 0
maximum(d::Poisson) = Inf
skewness(d::Poisson) = 1.0 / sqrt(d.λ)
kurtosis(d::Poisson) = 1.0 / d.λ

function pdf(d::Poisson, k::Real)
    k = Float64(k)
    if k < 0.0 || k != floor(k)
        return 0.0
    end
    return exp(k * log(d.λ) - d.λ - loggamma(k + 1.0))
end
function cdf(d::Poisson, k::Real)
    kk = floor(Float64(k))
    if kk < 0.0
        return 0.0
    end
    # P(X <= k) = Q(k+1, λ) = 1 - P(k+1, λ)
    return 1.0 - gamma_inc(kk + 1.0, d.λ)
end
quantile(d::Poisson, q::Real) =
    _discrete_quantile(d, q, 0, Int(floor(d.λ + 20.0 * sqrt(d.λ) + 20.0)))
mgf(d::Poisson, t::Real) = exp(d.λ * (exp(Float64(t)) - 1.0))
cf(d::Poisson, t::Real) = exp(d.λ * (cis(Float64(t)) - 1.0))
function kldivergence(p::Poisson, q::Poisson)
    return q.λ - p.λ + (p.λ > 0.0 ? p.λ * log(p.λ / q.λ) : 0.0)
end
# Knuth's multiplicative algorithm (suitable for small/moderate λ).
function rand(d::Poisson)
    el = exp(-d.λ)
    k = 0
    p = 1.0
    while true
        k += 1
        p *= rand()
        if p <= el
            return k - 1
        end
    end
end
function _rand_scalar(rng, d::Poisson)
    el = exp(-d.λ)
    k = 0
    p = 1.0
    while true
        k += 1
        p *= rand(rng)
        if p <= el
            return k - 1
        end
    end
end

# ── Geometric (number of failures before the first success) ─────────────────

struct Geometric{T<:Real} <: Distribution{Univariate, Discrete}
    p::T
end

function Geometric(p::Real)
    if p <= 0 || p > 1
        throw(ArgumentError("Geometric: the condition 0 < p <= 1 is not satisfied."))
    end
    v = float(p)
    return Geometric{typeof(v)}(v)
end
Geometric() = Geometric(0.5)

params(d::Geometric) = (d.p,)
succprob(d::Geometric) = d.p
failprob(d::Geometric) = 1.0 - d.p
mean(d::Geometric) = (1.0 - d.p) / d.p
var(d::Geometric) = (1.0 - d.p) / d.p^2
mode(d::Geometric) = 0
skewness(d::Geometric) = (2.0 - d.p) / sqrt(1.0 - d.p)
kurtosis(d::Geometric) = 6.0 + d.p^2 / (1.0 - d.p)
minimum(d::Geometric) = 0
maximum(d::Geometric) = Inf

function pdf(d::Geometric, k::Real)
    k = Float64(k)
    if k < 0.0 || k != floor(k)
        return 0.0
    end
    return d.p * (1.0 - d.p)^k
end
function cdf(d::Geometric, k::Real)
    kk = floor(Float64(k))
    if kk < 0.0
        return 0.0
    end
    return 1.0 - (1.0 - d.p)^(kk + 1.0)
end
function quantile(d::Geometric, q::Real)
    q = Float64(q)
    if q <= 0.0
        return 0
    end
    return Int(ceil(log(1.0 - q) / log(1.0 - d.p) - 1.0))
end
rand(d::Geometric) = Int(floor(log(1.0 - rand()) / log(1.0 - d.p)))
function cquantile(d::Geometric, p::Real)
    p = Float64(p)
    if p <= 0.0
        return Inf
    elseif p >= 1.0
        return 0
    end
    return Int(max(ceil(log(p) / log(1.0 - d.p)) - 1.0, 0.0))
end
invlogcdf(d::Geometric, lp::Real) = quantile(d, exp(Float64(lp)))
function invlogccdf(d::Geometric, lp::Real)
    lp = Float64(lp)
    if lp == 0.0
        return 0
    elseif lp == -Inf
        return Inf
    end
    return Int(max(ceil(lp / log(1.0 - d.p)) - 1.0, 0.0))
end
mgf(d::Geometric, t::Real) = d.p / (1.0 - (1.0 - d.p) * exp(Float64(t)))
cf(d::Geometric, t::Real) = d.p / (1.0 - (1.0 - d.p) * cis(Float64(t)))
function kldivergence(p::Geometric, q::Geometric)
    x = p.p
    y = q.p
    if x == y
        return 0.0
    elseif x == 1.0
        return -log(y / x)
    else
        return log(x) - log(y) + (1.0 / x - 1.0) * (log(1.0 - x) - log(1.0 - y))
    end
end
_rand_scalar(rng, d::Geometric) =
    Int(floor(log(1.0 - rand(rng)) / log(1.0 - d.p)))

# ── NegativeBinomial (failures before r successes) ─────────────────────────

struct NegativeBinomial{T<:Real} <: Distribution{Univariate, Discrete}
    r::T
    p::T
end

function NegativeBinomial(r::Real, p::Real)
    if r <= 0
        throw(ArgumentError("NegativeBinomial: the condition r > 0 is not satisfied."))
    end
    if p <= 0 || p > 1
        throw(ArgumentError("NegativeBinomial: the condition 0 < p <= 1 is not satisfied."))
    end
    rr, pp = promote(float(r), float(p))
    return NegativeBinomial{typeof(rr)}(rr, pp)
end
NegativeBinomial(r::Real) = NegativeBinomial(r, 0.5)
NegativeBinomial() = NegativeBinomial(1.0, 0.5)

params(d::NegativeBinomial) = (d.r, d.p)
succprob(d::NegativeBinomial) = d.p
failprob(d::NegativeBinomial) = 1.0 - d.p
mean(d::NegativeBinomial) = (1.0 - d.p) * d.r / d.p
var(d::NegativeBinomial) = (1.0 - d.p) * d.r / (d.p * d.p)
std(d::NegativeBinomial) = sqrt(var(d))
skewness(d::NegativeBinomial) = (2.0 - d.p) / sqrt((1.0 - d.p) * d.r)
kurtosis(d::NegativeBinomial) = 6.0 / d.r + d.p^2 / ((1.0 - d.p) * d.r)
mode(d::NegativeBinomial) = max(0, Int(floor((1.0 - d.p) * (d.r - 1.0) / d.p)))
minimum(d::NegativeBinomial) = 0
maximum(d::NegativeBinomial) = Inf
support(d::NegativeBinomial) = 0:typemax(Int64)

function logpdf(d::NegativeBinomial, k::Real)
    k = Float64(k)
    if k < 0.0 || k != floor(k)
        return -Inf
    end
    if d.p == 1.0
        return k == 0.0 ? 0.0 : -Inf
    end
    return loggamma(k + d.r) - loggamma(k + 1.0) - loggamma(d.r) +
           d.r * log(d.p) + k * log(1.0 - d.p)
end
pdf(d::NegativeBinomial, k::Real) = exp(logpdf(d, k))
function cdf(d::NegativeBinomial, k::Real)
    kk = floor(Float64(k))
    if kk < 0.0
        return 0.0
    end
    s = 0.0
    for i in 0:Int(kk)
        s += pdf(d, i)
    end
    return min(s, 1.0)
end
function quantile(d::NegativeBinomial, q::Real)
    p = Float64(q)
    if p <= 0.0
        return 0
    elseif p >= 1.0
        return typemax(Int64)
    end
    hi = 1
    while cdf(d, hi) < p
        hi *= 2
    end
    return _discrete_quantile(d, p, 0, hi)
end
function rand(d::NegativeBinomial)
    return _rand_scalar(Random.default_rng(), d)
end
function _rand_scalar(rng, d::NegativeBinomial)
    if d.p == 1.0
        return 0
    end
    λ = _rand_scalar(rng, Gamma(d.r, (1.0 - d.p) / d.p))
    return _rand_scalar(rng, Poisson(λ))
end

# ── DiscreteUniform ─────────────────────────────────────────────────────────

struct DiscreteUniform <: Distribution{Univariate, Discrete}
    a::Int
    b::Int
end

function DiscreteUniform(a::Integer, b::Integer)
    if a > b
        throw(ArgumentError("DiscreteUniform: the condition a <= b is not satisfied."))
    end
    return DiscreteUniform(Int(a), Int(b))
end
DiscreteUniform(b::Integer) = DiscreteUniform(0, b)
DiscreteUniform() = DiscreteUniform(0, 1)

params(d::DiscreteUniform) = (d.a, d.b)
span(d::DiscreteUniform) = d.b - d.a + 1
mean(d::DiscreteUniform) = (d.a + d.b) / 2.0
var(d::DiscreteUniform) = (span(d)^2 - 1.0) / 12.0
mode(d::DiscreteUniform) = d.a
modes(d::DiscreteUniform) = [d.a:d.b]
skewness(d::DiscreteUniform) = 0.0
function kurtosis(d::DiscreteUniform)
    n2 = span(d)^2
    return -1.2 * (n2 + 1.0) / (n2 - 1.0)
end
minimum(d::DiscreteUniform) = d.a
maximum(d::DiscreteUniform) = d.b

function pdf(d::DiscreteUniform, k::Real)
    kk = Float64(k)
    if kk < d.a || kk > d.b || kk != floor(kk)
        return 0.0
    end
    return 1.0 / span(d)
end
function cdf(d::DiscreteUniform, k::Real)
    kk = floor(Float64(k))
    if kk < d.a
        return 0.0
    elseif kk >= d.b
        return 1.0
    end
    return (kk - d.a + 1.0) / span(d)
end
function quantile(d::DiscreteUniform, q::Real)
    q = Float64(q)
    if q <= 0.0
        return d.a
    elseif q >= 1.0
        return d.b
    end
    return d.a + Int(ceil(q * span(d))) - 1
end
rand(d::DiscreteUniform) = d.a + Int(floor(span(d) * rand()))
function mgf(d::DiscreteUniform, t::Real)
    t = Float64(t)
    if t == 0.0
        return 1.0
    end
    u = span(d)
    return (exp(t * d.a) * expm1(t * u)) / (u * expm1(t))
end
function cf(d::DiscreteUniform, t::Real)
    t = Float64(t)
    if t == 0.0
        return Complex{Float64}(1.0, 0.0)
    end
    u = span(d)
    return (im * cos(t * (d.a + d.b) / 2.0) + sin(t * (d.a - d.b - 1.0) / 2.0)) /
           (u * sin(t / 2.0))
end
_rand_scalar(rng, d::DiscreteUniform) = d.a + Int(floor(span(d) * rand(rng)))

# ── Hypergeometric ──────────────────────────────────────────────────────────

struct Hypergeometric <: Distribution{Univariate, Discrete}
    ns::Int
    nf::Int
    n::Int
end

function Hypergeometric(ns::Integer, nf::Integer, n::Integer)
    if ns < 0 || nf < 0
        throw(ArgumentError("Hypergeometric: ns and nf must be nonnegative."))
    end
    if n < 0 || n > ns + nf
        throw(ArgumentError("Hypergeometric: n must satisfy 0 <= n <= ns + nf."))
    end
    return Hypergeometric(Int(ns), Int(nf), Int(n))
end

params(d::Hypergeometric) = (d.ns, d.nf, d.n)
minimum(d::Hypergeometric) = max(0, d.n - d.nf)
maximum(d::Hypergeometric) = min(d.ns, d.n)
support(d::Hypergeometric) = minimum(d):maximum(d)
mean(d::Hypergeometric) = d.n * d.ns / (d.ns + d.nf)
function var(d::Hypergeometric)
    total = d.ns + d.nf
    if total <= 1
        return 0.0
    end
    p = d.ns / total
    return d.n * p * (1.0 - p) * (total - d.n) / (total - 1.0)
end
function mode(d::Hypergeometric)
    m = Int(floor((d.n + 1.0) * (d.ns + 1.0) / (d.ns + d.nf + 2.0)))
    return min(max(m, minimum(d)), maximum(d))
end

function logpdf(d::Hypergeometric, k::Real)
    k = Float64(k)
    if k < minimum(d) || k > maximum(d) || k != floor(k)
        return -Inf
    end
    return _logbinom(d.ns, k) + _logbinom(d.nf, d.n - k) - _logbinom(d.ns + d.nf, d.n)
end
pdf(d::Hypergeometric, k::Real) = exp(logpdf(d, k))
function cdf(d::Hypergeometric, k::Real)
    kk = floor(Float64(k))
    if kk < minimum(d)
        return 0.0
    elseif kk >= maximum(d)
        return 1.0
    end
    s = 0.0
    for i in minimum(d):Int(kk)
        s += pdf(d, i)
    end
    return min(s, 1.0)
end
quantile(d::Hypergeometric, q::Real) = _discrete_quantile(d, q, minimum(d), maximum(d))
rand(d::Hypergeometric) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::Hypergeometric)
    successes_left = d.ns
    failures_left = d.nf
    hits = 0
    for _ in 1:d.n
        total = successes_left + failures_left
        if total > 0 && rand(rng) < successes_left / total
            hits += 1
            successes_left -= 1
        else
            failures_left -= 1
        end
    end
    return hits
end

# ── BetaBinomial ────────────────────────────────────────────────────────────

struct BetaBinomial{T<:Real} <: Distribution{Univariate, Discrete}
    n::Int
    α::T
    β::T
end

function BetaBinomial(n::Integer, α::Real, β::Real)
    if n < 0
        throw(ArgumentError("BetaBinomial: the condition n >= 0 is not satisfied."))
    end
    if α <= 0 || β <= 0
        throw(ArgumentError("BetaBinomial: the conditions α > 0 and β > 0 are not satisfied."))
    end
    a, b = promote(float(α), float(β))
    return BetaBinomial{typeof(a)}(Int(n), a, b)
end

params(d::BetaBinomial) = (d.n, d.α, d.β)
ntrials(d::BetaBinomial) = d.n
minimum(d::BetaBinomial) = 0
maximum(d::BetaBinomial) = d.n
support(d::BetaBinomial) = 0:d.n
mean(d::BetaBinomial) = d.n * d.α / (d.α + d.β)
function var(d::BetaBinomial)
    s = d.α + d.β
    return d.n * d.α * d.β * (s + d.n) / (s * s * (s + 1.0))
end
function skewness(d::BetaBinomial)
    s = d.α + d.β
    t1 = (s + 2.0 * d.n) * (d.β - d.α) / (s + 2.0)
    t2 = sqrt((1.0 + s) / (d.n * d.α * d.β * (d.n + s)))
    return t1 * t2
end
function kurtosis(d::BetaBinomial)
    n = d.n
    a = d.α
    b = d.β
    s = a + b
    ab = a * b
    left = s^2 * (1.0 + s) / (n * ab * (s + 2.0) * (s + 3.0) * (s + n))
    right = s * (s - 1.0 + 6.0 * n) + 3.0 * ab * (n - 2.0) + 6.0 * n^2
    right -= (3.0 * ab * n * (6.0 - n)) / s
    right -= (18.0 * ab * n^2) / (s * s)
    return left * right - 3.0
end
function logpdf(d::BetaBinomial, k::Real)
    k = Float64(k)
    if k < 0.0 || k > d.n || k != floor(k)
        return -Inf
    end
    return _logbinom(d.n, k) + lbeta(k + d.α, d.n - k + d.β) - lbeta(d.α, d.β)
end
pdf(d::BetaBinomial, k::Real) = exp(logpdf(d, k))
function cdf(d::BetaBinomial, k::Real)
    kk = floor(Float64(k))
    if kk < 0.0
        return 0.0
    elseif kk >= d.n
        return 1.0
    end
    s = 0.0
    for i in 0:Int(kk)
        s += pdf(d, i)
    end
    return min(s, 1.0)
end
quantile(d::BetaBinomial, q::Real) = _discrete_quantile(d, q, 0, d.n)
function mode(d::BetaBinomial)
    best = 0
    bestp = pdf(d, 0)
    for k in 1:d.n
        pk = pdf(d, k)
        if pk > bestp
            bestp = pk
            best = k
        end
    end
    return best
end
function modes(d::BetaBinomial)
    bestp = pdf(d, 0)
    out = Int64[0]
    for k in 1:d.n
        pk = pdf(d, k)
        if pk > bestp
            bestp = pk
            out = Int64[k]
        elseif pk == bestp
            push!(out, k)
        end
    end
    return out
end
rand(d::BetaBinomial) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::BetaBinomial)
    p = _rand_scalar(rng, Beta(d.α, d.β))
    return _rand_scalar(rng, Binomial(d.n, p))
end

# ── Skellam ─────────────────────────────────────────────────────────────────

struct Skellam{T<:Real} <: Distribution{Univariate, Discrete}
    μ1::T
    μ2::T
end

function Skellam(μ1::Real, μ2::Real)
    if μ1 <= 0 || μ2 <= 0
        throw(ArgumentError("Skellam: the conditions μ1 > 0 and μ2 > 0 are not satisfied."))
    end
    a, b = promote(float(μ1), float(μ2))
    return Skellam{typeof(a)}(a, b)
end
Skellam(μ::Real) = Skellam(μ, μ)
Skellam() = Skellam(1.0, 1.0)

params(d::Skellam) = (d.μ1, d.μ2)
mean(d::Skellam) = d.μ1 - d.μ2
var(d::Skellam) = d.μ1 + d.μ2
skewness(d::Skellam) = mean(d) / sqrt(var(d))^3
kurtosis(d::Skellam) = 1.0 / var(d)
minimum(d::Skellam) = -Inf
maximum(d::Skellam) = Inf
support(d::Skellam) = (-Inf, Inf)
insupport(d::Skellam, x::Real) = _is_integer_value(x)

function _besseli_int(n::Int, x::Real)
    order = abs(n)
    z = Float64(x)
    if z == 0.0
        return order == 0 ? 1.0 : 0.0
    end
    half = z / 2.0
    term = half^order / gamma(order + 1.0)
    s = term
    m = 1
    while m <= 200
        term *= (half * half) / (m * (m + order))
        s += term
        if abs(term) <= 1.0e-14 * abs(s)
            return s
        end
        m += 1
    end
    return s
end

function pdf(d::Skellam, k::Real)
    x = Float64(k)
    if !_is_integer_value(x)
        return 0.0
    end
    kk = Int(x)
    z = 2.0 * sqrt(d.μ1 * d.μ2)
    return exp(-(d.μ1 + d.μ2)) * (d.μ1 / d.μ2)^(x / 2.0) *
           _besseli_int(abs(kk), z)
end
function logpdf(d::Skellam, k::Real)
    p = pdf(d, k)
    return p == 0.0 ? -Inf : log(p)
end
function _skellam_cdf_lower(d::Skellam, upto::Int)
    center = min(Float64(upto), mean(d))
    return Int(floor(center - 14.0 * sqrt(var(d)) - 20.0))
end
function cdf(d::Skellam, k::Real)
    x = Float64(k)
    if x == -Inf
        return 0.0
    elseif x == Inf
        return 1.0
    end
    kk = Int(floor(x))
    lo = _skellam_cdf_lower(d, kk)
    s = 0.0
    for i in lo:kk
        s += pdf(d, i)
    end
    return min(max(s, 0.0), 1.0)
end
function mode(d::Skellam)
    σ = sqrt(var(d))
    lo = Int(floor(mean(d) - 8.0 * σ - 10.0))
    hi = Int(ceil(mean(d) + 8.0 * σ + 10.0))
    best = lo
    bestp = pdf(d, lo)
    for k in (lo + 1):hi
        pk = pdf(d, k)
        if pk > bestp
            bestp = pk
            best = k
        end
    end
    return best
end
function quantile(d::Skellam, q::Real)
    p = Float64(q)
    if p <= 0.0
        return -typemax(Int64)
    elseif p >= 1.0
        return typemax(Int64)
    end
    σ = sqrt(var(d))
    lo = Int(floor(mean(d) - 14.0 * σ - 20.0))
    hi = Int(ceil(mean(d) + 14.0 * σ + 20.0))
    step = max(10, Int(ceil(8.0 * σ + 10.0)))
    while cdf(d, hi) < p
        hi += step
        step *= 2
    end
    return _discrete_quantile(d, p, lo, hi)
end
rand(d::Skellam) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Skellam) =
    _rand_scalar(rng, Poisson(d.μ1)) - _rand_scalar(rng, Poisson(d.μ2))

# ── Dirac ───────────────────────────────────────────────────────────────────

struct Dirac{T<:Real} <: Distribution{Univariate, Discrete}
    value::T
end

params(d::Dirac) = (d.value,)
mean(d::Dirac) = d.value
median(d::Dirac) = d.value
mode(d::Dirac) = d.value
var(d::Dirac) = zero(d.value)
skewness(d::Dirac) = zero(d.value)
kurtosis(d::Dirac) = zero(d.value)
entropy(d::Dirac) = zero(d.value)
minimum(d::Dirac) = d.value
maximum(d::Dirac) = d.value
support(d::Dirac) = (d.value,)
insupport(d::Dirac, x::Real) = x == d.value
pdf(d::Dirac, x::Real) = insupport(d, x) ? 1.0 : 0.0
logpdf(d::Dirac, x::Real) = insupport(d, x) ? 0.0 : -Inf
function cdf(d::Dirac, x::Real)
    return x < d.value ? 0.0 : 1.0
end
function quantile(d::Dirac, q::Real)
    p = Float64(q)
    return 0.0 <= p <= 1.0 ? d.value : NaN
end
mgf(d::Dirac, t::Real) = exp(Float64(t) * d.value)
cf(d::Dirac, t::Real) = cis(Float64(t) * d.value)
rand(d::Dirac) = _rand_scalar(Random.default_rng(), d)
_rand_scalar(rng, d::Dirac) = d.value

# ── PoissonBinomial ─────────────────────────────────────────────────────────

struct PoissonBinomial{T<:Real} <: Distribution{Univariate, Discrete}
    p::Vector{T}
end

function PoissonBinomial(p::AbstractVector{<:Real})
    v = [float(x) for x in p]
    for x in v
        if x < 0 || x > 1
            throw(ArgumentError("PoissonBinomial: probabilities must satisfy 0 <= p <= 1."))
        end
    end
    return PoissonBinomial{eltype(v)}(v)
end

params(d::PoissonBinomial) = (d.p,)
probs(d::PoissonBinomial) = d.p
succprob(d::PoissonBinomial) = d.p
failprob(d::PoissonBinomial) = [1.0 - x for x in d.p]
ntrials(d::PoissonBinomial) = length(d.p)
minimum(d::PoissonBinomial) = 0
maximum(d::PoissonBinomial) = ntrials(d)
support(d::PoissonBinomial) = 0:ntrials(d)
mean(d::PoissonBinomial) = sum(d.p)
function var(d::PoissonBinomial)
    s = 0.0
    for p in d.p
        s += p * (1.0 - p)
    end
    return s
end

function _poissonbinomial_pdf_values(p)
    n = length(p)
    s = zeros(Float64, n + 1)
    s[1] = 1.0
    col = 1
    while col <= n
        pc = p[col]
        qc = 1.0 - pc
        row = col
        while row >= 1
            s[row + 1] = qc * s[row + 1] + pc * s[row]
            row -= 1
        end
        s[1] *= qc
        col += 1
    end
    return s
end

function pdf(d::PoissonBinomial, k::Real)
    x = Float64(k)
    if x < 0.0 || x > ntrials(d) || x != floor(x)
        return 0.0
    end
    return _poissonbinomial_pdf_values(d.p)[Int(x) + 1]
end
logpdf(d::PoissonBinomial, k::Real) = log(pdf(d, k))
function cdf(d::PoissonBinomial, k::Real)
    kk = floor(Float64(k))
    if kk < 0.0
        return 0.0
    elseif kk >= ntrials(d)
        return 1.0
    end
    values = _poissonbinomial_pdf_values(d.p)
    s = 0.0
    for i in 0:Int(kk)
        s += values[i + 1]
    end
    return min(s, 1.0)
end
quantile(d::PoissonBinomial, q::Real) =
    _discrete_quantile(d, q, 0, ntrials(d))
function mode(d::PoissonBinomial)
    values = _poissonbinomial_pdf_values(d.p)
    best = 0
    bestp = values[1]
    for k in 1:ntrials(d)
        pk = values[k + 1]
        if pk > bestp
            bestp = pk
            best = k
        end
    end
    return best
end
function modes(d::PoissonBinomial)
    values = _poissonbinomial_pdf_values(d.p)
    bestp = values[1]
    out = Int64[0]
    for k in 1:ntrials(d)
        pk = values[k + 1]
        if pk > bestp
            bestp = pk
            out = Int64[k]
        elseif pk == bestp
            previous = out
            out = Int64[k]
            for v in previous
                push!(out, v)
            end
        end
    end
    return out
end
function skewness(d::PoissonBinomial)
    v = var(d)
    s = 0.0
    for p in d.p
        s += p * (1.0 - p) * (1.0 - 2.0 * p)
    end
    return s / sqrt(v)^3
end
function kurtosis(d::PoissonBinomial)
    v = var(d)
    s = 0.0
    for p in d.p
        s += p * (1.0 - p) * (1.0 - 6.0 * p * (1.0 - p))
    end
    return s / v^2
end
function mgf(d::PoissonBinomial, t::Real)
    et = exp(Float64(t))
    prod = 1.0
    for p in d.p
        prod *= 1.0 - p + p * et
    end
    return prod
end
function cf(d::PoissonBinomial, t::Real)
    z = cis(Float64(t))
    prod = Complex{Float64}(1.0, 0.0)
    for p in d.p
        prod *= 1.0 - p + p * z
    end
    return prod
end
rand(d::PoissonBinomial) = _rand_scalar(Random.default_rng(), d)
function _rand_scalar(rng, d::PoissonBinomial)
    c = 0
    for p in d.p
        if rand(rng) < p
            c += 1
        end
    end
    return c
end

# ── Categorical (distribution over 1:k with given probabilities) ─────────────
# Issue #7260. Support is the integer range 1:ncategories(d); `p[i]` is the
# probability of outcome `i`. `ncategories(d)` is `length(d.p)`.
#
# Upstream-faithful parametric form: `Categorical{T<:Real}` with a typed
# `p::Vector{T}` field. The typed methods (`var(d::Categorical)`,
# `mean`/`mode`/`quantile`, the module-local `ncategories`, …) now reliably beat
# the untyped `Statistics.var(arr)` / `Statistics.median(arr)` generics they
# extend (Issues #7263 / #7265): the fix lets a within-module method annotation
# on a module/package struct match a within-module argument of the same family
# regardless of module qualification.
#
# `Categorical(k::Integer)` (uniform over 1:k) uses the natural upstream form
# `Categorical([1.0/k for _ in 1:k])`. Issue #7266 fixed the comprehension
# argument loose-matching this `::Integer` method (the #5966 abstract-annotation
# dispatch family) — `Categorical(3)` no longer evaluates `1:Array` — so the
# inner re-dispatch now correctly selects the 1-arg
# `Categorical(p::AbstractVector{<:Real})` constructor.

struct Categorical{T<:Real} <: Distribution{Univariate, Discrete}
    p::Vector{T}
end

function Categorical(p::AbstractVector{<:Real})
    if isempty(p)
        throw(ArgumentError("Categorical: the probability vector must be non-empty."))
    end
    s = 0.0
    for x in p
        if x < 0
            throw(ArgumentError("Categorical: probabilities must be non-negative."))
        end
        s += float(x)
    end
    if abs(s - 1.0) > 1.0e-8
        throw(ArgumentError("Categorical: probabilities must sum to 1."))
    end
    v = [float(x) for x in p]
    return Categorical{eltype(v)}(v)
end
# Uniform over 1:k. With #7263/#7265 (within-module dispatch) and #7266
# (comprehension no longer loose-matches ::Integer) both fixed, the natural
# upstream form works again: the comprehension routes to the 1-arg
# `Categorical(p::AbstractVector{<:Real})` constructor.
Categorical(k::Integer) = Categorical([1.0 / k for _ in 1:k])

params(d::Categorical) = (d.p,)
probs(d::Categorical) = d.p
ncategories(d::Categorical) = length(d.p)
support(d::Categorical) = 1:ncategories(d)
minimum(d::Categorical) = 1
maximum(d::Categorical) = ncategories(d)

function mean(d::Categorical)
    m = 0.0
    for i in 1:ncategories(d)
        m += i * d.p[i]
    end
    return m
end
function var(d::Categorical)
    m = mean(d)
    v = 0.0
    for i in 1:ncategories(d)
        v += (i - m)^2 * d.p[i]
    end
    return v
end
function mode(d::Categorical)
    best = 1
    bestp = d.p[1]
    for i in 2:ncategories(d)
        if d.p[i] > bestp
            bestp = d.p[i]
            best = i
        end
    end
    return best
end
function modes(d::Categorical)
    bestp = d.p[1]
    out = Int64[]
    for i in 1:ncategories(d)
        if d.p[i] > bestp
            bestp = d.p[i]
            out = Int64[i]
        elseif d.p[i] == bestp
            push!(out, i)
        end
    end
    return out
end
function skewness(d::Categorical)
    m = mean(d)
    σ = sqrt(var(d))
    s = 0.0
    for i in 1:ncategories(d)
        s += (i - m)^3 * d.p[i]
    end
    return s / σ^3
end
function kurtosis(d::Categorical)
    m = mean(d)
    v = var(d)
    s = 0.0
    for i in 1:ncategories(d)
        s += (i - m)^4 * d.p[i]
    end
    return s / v^2 - 3.0
end
function entropy(d::Categorical)
    s = 0.0
    for x in d.p
        if x > 0.0
            s -= x * log(x)
        end
    end
    return s
end

function pdf(d::Categorical, k::Real)
    kk = Float64(k)
    if kk < 1.0 || kk > ncategories(d) || kk != floor(kk)
        return 0.0
    end
    return d.p[Int(kk)]
end
function cdf(d::Categorical, k::Real)
    kk = floor(Float64(k))
    if kk < 1.0
        return 0.0
    end
    if kk >= ncategories(d)
        return 1.0
    end
    c = 0.0
    for i in 1:Int(kk)
        c += d.p[i]
    end
    return c
end
quantile(d::Categorical, q::Real) = _discrete_quantile(d, q, 1, ncategories(d))
function mgf(d::Categorical, t::Real)
    s = 0.0
    for i in 1:ncategories(d)
        s += d.p[i] * exp(Float64(t) * i)
    end
    return s
end
function cf(d::Categorical, t::Real)
    s = Complex{Float64}(0.0, 0.0)
    for i in 1:ncategories(d)
        s += d.p[i] * cis(Float64(t) * i)
    end
    return s
end
function kldivergence(p::Categorical, q::Categorical)
    if ncategories(p) != ncategories(q)
        error("kldivergence: Categorical distributions must have the same number of categories")
    end
    s = 0.0
    for i in 1:ncategories(p)
        s += _xlogratio(p.p[i], q.p[i])
    end
    return s
end
# Inverse-CDF sampling using the global RNG.
function rand(d::Categorical)
    u = rand()
    c = 0.0
    nc = ncategories(d)
    for i in 1:nc
        c += d.p[i]
        if u <= c
            return i
        end
    end
    return nc
end
function _rand_scalar(rng, d::Categorical)
    u = rand(rng)
    c = 0.0
    nc = ncategories(d)
    for i in 1:nc
        c += d.p[i]
        if u <= c
            return i
        end
    end
    return nc
end
