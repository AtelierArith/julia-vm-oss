# Maximum-likelihood fitting (Issues #7178, #7326).
#
# `fit_mle(D, x)` returns the maximum-likelihood estimate of distribution `D`
# from data `x`; `fit(D, x)` usually delegates to MLE.
#
# DISPATCH NOTE (Issue #7247): the natural `fit_mle(::Type{Normal}, x)` form does
# not work here — a parametric type with user-defined constructors (Normal{T},
# Bernoulli{T}, …) is not matched by `::Type{T}` dispatch. So public fitting
# entry points take an untyped distribution argument and branch on type identity.

struct NormalStats
    m::Float64
    s2::Float64
    tw::Float64
end

struct UniformStats
    lo::Float64
    hi::Float64
end

struct ExponentialStats
    sx::Float64
    sw::Float64
end

struct GammaStats
    sx::Float64
    slogx::Float64
    sw::Float64
end

struct BetaStats
    sx::Float64
    sx2::Float64
    slogx::Float64
    slog1mx::Float64
    sw::Float64
end

struct LogNormalStats
    m::Float64
    s2::Float64
    tw::Float64
end

struct WeibullStats
    x::Vector{Float64}
end

struct CauchyStats
    q25::Float64
    q50::Float64
    q75::Float64
end

struct BernoulliStats
    cnt1::Float64
    cnt0::Float64
end

struct BinomialStats
    n::Int
    ns::Float64
    ne::Float64
end

struct PoissonStats
    sx::Float64
    sw::Float64
end

struct GeometricStats
    sx::Float64
    sw::Float64
end

struct CategoricalStats
    counts::Vector{Float64}
    sw::Float64
end

function _check_nonempty(x)
    if length(x) == 0
        error("fit_mle: data must be non-empty")
    end
end

function _check_positive_data(D, x)
    for v in x
        if v <= 0
            error("fit_mle: $(D) requires positive observations")
        end
    end
end

function _check_unit_interval_data(D, x)
    for v in x
        if v <= 0 || v >= 1
            error("fit_mle: $(D) requires observations in (0, 1)")
        end
    end
end

function _datamean(x)
    _check_nonempty(x)
    s = 0.0
    for v in x
        s += v
    end
    return s / length(x)
end

function _population_s2(x, m)
    s = 0.0
    for v in x
        d = Float64(v) - m
        s += d * d
    end
    return s
end

function _copy_float_vector(x)
    out = Float64[]
    for v in x
        push!(out, Float64(v))
    end
    return out
end

function _sort_float_vector(x)
    out = _copy_float_vector(x)
    n = length(out)
    for i in 2:n
        v = out[i]
        j = i - 1
        while j >= 1 && out[j] > v
            out[j + 1] = out[j]
            j -= 1
        end
        out[j + 1] = v
    end
    return out
end

function _sample_quantile(sorted_x, p::Real)
    n = length(sorted_x)
    if n == 1
        return sorted_x[1]
    end
    h = 1.0 + (n - 1.0) * Float64(p)
    lo = Int(floor(h))
    hi = Int(ceil(h))
    if lo == hi
        return sorted_x[lo]
    end
    w = h - lo
    return sorted_x[lo] * (1.0 - w) + sorted_x[hi] * w
end

function _digamma_fit(x::Real)
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

function _trigamma_fit(x::Real)
    y = Float64(x)
    result = 0.0
    while y < 8.0
        result += 1.0 / (y * y)
        y += 1.0
    end
    inv = 1.0 / y
    inv2 = inv * inv
    inv3 = inv2 * inv
    inv5 = inv3 * inv2
    inv7 = inv5 * inv2
    return result + inv + 0.5 * inv2 + inv3 / 6.0 - inv5 / 30.0 + inv7 / 42.0
end

function _fit_gamma_stats(ss::GammaStats)
    meanx = ss.sx / ss.sw
    s = log(meanx) - ss.slogx / ss.sw
    if s <= 0.0
        return Gamma(1.0e12, meanx / 1.0e12)
    end
    a = (3.0 - s + sqrt((s - 3.0)^2 + 24.0 * s)) / (12.0 * s)
    for _ in 1:100
        f = log(a) - _digamma_fit(a) - s
        fp = 1.0 / a - _trigamma_fit(a)
        step = f / fp
        next = a - step
        if next <= 0.0 || !isfinite(next)
            next = a / 2.0
        end
        if abs(next - a) <= 1.0e-12 * (abs(a) + 1.0)
            a = next
            break
        end
        a = next
    end
    return Gamma(a, meanx / a)
end

function _fit_beta_moments(ss::BetaStats)
    m = ss.sx / ss.sw
    if ss.sw <= 1.0
        error("fit: Beta data variance must be positive")
    end
    v = (ss.sx2 - ss.sx * ss.sx / ss.sw) / (ss.sw - 1.0)
    if v <= 0.0
        error("fit: Beta data variance must be positive")
    end
    tmp = m * (1.0 - m) / v - 1.0
    return Beta(m * tmp, (1.0 - m) * tmp)
end

function _fit_beta_mle_stats(ss::BetaStats)
    d0 = _fit_beta_moments(ss)
    a = Float64(d0.α)
    b = Float64(d0.β)
    meanlogx = ss.slogx / ss.sw
    meanlog1mx = ss.slog1mx / ss.sw
    for _ in 1:100
        ab = a + b
        g1 = meanlogx + _digamma_fit(ab) - _digamma_fit(a)
        g2 = meanlog1mx + _digamma_fit(ab) - _digamma_fit(b)
        h11 = _trigamma_fit(ab) - _trigamma_fit(a)
        h22 = _trigamma_fit(ab) - _trigamma_fit(b)
        h12 = _trigamma_fit(ab)
        det = h11 * h22 - h12 * h12
        da = (h22 * g1 - h12 * g2) / det
        db = (-h12 * g1 + h11 * g2) / det
        next_a = a - da
        next_b = b - db
        if next_a <= 0.0 || next_b <= 0.0 || !isfinite(next_a) || !isfinite(next_b)
            next_a = 0.5 * (a + Float64(d0.α))
            next_b = 0.5 * (b + Float64(d0.β))
        end
        if da * da + db * db < 1.0e-20
            a = next_a
            b = next_b
            break
        end
        a = next_a
        b = next_b
    end
    return Beta(a, b)
end

function _fit_weibull_stats(ss::WeibullStats)
    x = ss.x
    n = length(x)
    meanlog = 0.0
    for v in x
        meanlog += log(v)
    end
    meanlog /= n
    alpha = 1.0
    for _ in 1:100
        sx = 0.0
        sxlog = 0.0
        sxlog2 = 0.0
        for v in x
            lv = log(v)
            xa = v^alpha
            sx += xa
            sxlog += xa * lv
            sxlog2 += xa * lv * lv
        end
        f = sxlog / sx - meanlog - 1.0 / alpha
        fp = (sx * sxlog2 - sxlog * sxlog) / (sx * sx) + 1.0 / (alpha * alpha)
        step = f / fp
        next = alpha - step
        if next <= 0.0 || !isfinite(next)
            next = alpha / 2.0
        end
        if abs(next - alpha) <= 1.0e-12 * (abs(alpha) + 1.0)
            alpha = next
            break
        end
        alpha = next
    end
    sx = 0.0
    for v in x
        sx += v^alpha
    end
    theta = (sx / n)^(1.0 / alpha)
    return Weibull(alpha, theta)
end

function suffstats(D, x)
    _check_nonempty(x)
    if D === Normal
        m = _datamean(x)
        return NormalStats(m, _population_s2(x, m), Float64(length(x)))
    elseif D === Uniform
        lo = Float64(x[1])
        hi = Float64(x[1])
        for v in x
            fv = Float64(v)
            if fv < lo
                lo = fv
            end
            if fv > hi
                hi = fv
            end
        end
        return UniformStats(lo, hi)
    elseif D === Exponential
        _check_positive_data(D, x)
        sx = 0.0
        for v in x
            sx += v
        end
        return ExponentialStats(sx, Float64(length(x)))
    elseif D === Gamma
        _check_positive_data(D, x)
        sx = 0.0
        slogx = 0.0
        for v in x
            sx += v
            slogx += log(v)
        end
        return GammaStats(sx, slogx, Float64(length(x)))
    elseif D === Beta
        _check_unit_interval_data(D, x)
        sx = 0.0
        sx2 = 0.0
        slogx = 0.0
        slog1mx = 0.0
        for v in x
            fv = Float64(v)
            sx += fv
            sx2 += fv * fv
            slogx += log(fv)
            slog1mx += log(1.0 - fv)
        end
        return BetaStats(sx, sx2, slogx, slog1mx, Float64(length(x)))
    elseif D === LogNormal
        _check_positive_data(D, x)
        logs = Float64[]
        for v in x
            push!(logs, log(v))
        end
        m = _datamean(logs)
        return LogNormalStats(m, _population_s2(logs, m), Float64(length(logs)))
    elseif D === Weibull
        _check_positive_data(D, x)
        return WeibullStats(_copy_float_vector(x))
    elseif D === Cauchy
        sx = _sort_float_vector(x)
        return CauchyStats(
            _sample_quantile(sx, 0.25),
            _sample_quantile(sx, 0.5),
            _sample_quantile(sx, 0.75),
        )
    elseif D === Bernoulli
        cnt1 = 0.0
        cnt0 = 0.0
        for v in x
            if v == 1 || v == 1.0
                cnt1 += 1.0
            elseif v == 0 || v == 0.0
                cnt0 += 1.0
            else
                error("fit_mle: Bernoulli observations must be 0 or 1")
            end
        end
        return BernoulliStats(cnt1, cnt0)
    elseif D === Poisson
        sx = 0.0
        for v in x
            if v < 0 || v != floor(v)
                error("fit_mle: Poisson observations must be non-negative integers")
            end
            sx += v
        end
        return PoissonStats(sx, Float64(length(x)))
    elseif D === Geometric
        sx = 0.0
        for v in x
            if v < 0 || v != floor(v)
                error("fit_mle: Geometric observations must be non-negative integers")
            end
            sx += v
        end
        return GeometricStats(sx, Float64(length(x)))
    elseif D === Categorical
        k = 0
        for v in x
            if v < 1 || v != floor(v)
                error("fit_mle: Categorical observations must be positive integers")
            end
            if Int(v) > k
                k = Int(v)
            end
        end
        return suffstats(D, k, x)
    else
        error("suffstats: unsupported distribution")
    end
end

function suffstats(D, n::Integer, x)
    _check_nonempty(x)
    if D === Binomial
        ns = 0.0
        for v in x
            if v < 0 || v > n || v != floor(v)
                error("fit_mle: Binomial observations must be integers in 0:n")
            end
            ns += v
        end
        return BinomialStats(Int(n), ns, Float64(length(x)))
    elseif D === Categorical
        if n <= 0
            error("fit_mle: Categorical category count must be positive")
        end
        counts = zeros(Int(n))
        total = 0.0
        for v in x
            if v < 1 || v > n || v != floor(v)
                error("fit_mle: Categorical observations must be integers in 1:k")
            end
            counts[Int(v)] += 1.0
            total += 1.0
        end
        return CategoricalStats(counts, total)
    else
        error("suffstats: unsupported distribution/data signature")
    end
end

fit_mle(D, ss::NormalStats) = D === Normal ? Normal(ss.m, sqrt(ss.s2 / ss.tw)) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::UniformStats) = D === Uniform ? Uniform(ss.lo, ss.hi) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::ExponentialStats) = D === Exponential ? Exponential(ss.sx / ss.sw) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::GammaStats) = D === Gamma ? _fit_gamma_stats(ss) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::BetaStats) = D === Beta ? _fit_beta_mle_stats(ss) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::LogNormalStats) = D === LogNormal ? LogNormal(ss.m, sqrt(ss.s2 / (ss.tw - 1.0))) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::WeibullStats) = D === Weibull ? _fit_weibull_stats(ss) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::CauchyStats) = D === Cauchy ? Cauchy(ss.q50, (ss.q75 - ss.q25) / 2.0) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::BernoulliStats) = D === Bernoulli ? Bernoulli(ss.cnt1 / (ss.cnt0 + ss.cnt1)) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::BinomialStats) = D === Binomial ? Binomial(ss.n, ss.ns / (ss.ne * ss.n)) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::PoissonStats) = D === Poisson ? Poisson(ss.sx / ss.sw) : error("fit_mle: unsupported sufficient statistics")
fit_mle(D, ss::GeometricStats) = D === Geometric ? Geometric(1.0 / (1.0 + ss.sx / ss.sw)) : error("fit_mle: unsupported sufficient statistics")
function fit_mle(D, ss::CategoricalStats)
    if D !== Categorical
        error("fit_mle: unsupported sufficient statistics")
    end
    p = Float64[]
    for c in ss.counts
        push!(p, c / ss.sw)
    end
    return Categorical(p)
end

function fit_mle(D, x)
    if x isa NormalStats || x isa UniformStats || x isa ExponentialStats ||
       x isa GammaStats || x isa BetaStats || x isa LogNormalStats ||
       x isa WeibullStats || x isa CauchyStats || x isa BernoulliStats ||
       x isa BinomialStats || x isa PoissonStats || x isa GeometricStats ||
       x isa CategoricalStats
        return _fit_mle_from_stats(D, x)
    end
    if D === Normal || D === Uniform || D === Exponential || D === Gamma ||
       D === Beta || D === LogNormal || D === Weibull || D === Cauchy ||
       D === Bernoulli || D === Poisson || D === Geometric || D === Categorical
        return fit_mle(D, suffstats(D, x))
    elseif D === MvNormal
        return _fit_mvnormal(x)
    else
        error("fit_mle: unsupported distribution")
    end
end

function _fit_mle_from_stats(D, ss)
    if ss isa NormalStats
        return D === Normal ? Normal(ss.m, sqrt(ss.s2 / ss.tw)) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa UniformStats
        return D === Uniform ? Uniform(ss.lo, ss.hi) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa ExponentialStats
        return D === Exponential ? Exponential(ss.sx / ss.sw) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa GammaStats
        return D === Gamma ? _fit_gamma_stats(ss) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa BetaStats
        return D === Beta ? _fit_beta_mle_stats(ss) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa LogNormalStats
        return D === LogNormal ? LogNormal(ss.m, sqrt(ss.s2 / (ss.tw - 1.0))) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa WeibullStats
        return D === Weibull ? _fit_weibull_stats(ss) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa CauchyStats
        return D === Cauchy ? Cauchy(ss.q50, (ss.q75 - ss.q25) / 2.0) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa BernoulliStats
        return D === Bernoulli ? Bernoulli(ss.cnt1 / (ss.cnt0 + ss.cnt1)) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa BinomialStats
        return D === Binomial ? Binomial(ss.n, ss.ns / (ss.ne * ss.n)) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa PoissonStats
        return D === Poisson ? Poisson(ss.sx / ss.sw) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa GeometricStats
        return D === Geometric ? Geometric(1.0 / (1.0 + ss.sx / ss.sw)) : error("fit_mle: unsupported sufficient statistics")
    elseif ss isa CategoricalStats
        if D !== Categorical
            error("fit_mle: unsupported sufficient statistics")
        end
        p = Float64[]
        for c in ss.counts
            push!(p, c / ss.sw)
        end
        return Categorical(p)
    end
    error("fit_mle: unsupported sufficient statistics")
end

function fit_mle(D, n::Integer, x)
    if D === Binomial || D === Categorical
        return fit_mle(D, suffstats(D, n, x))
    else
        error("fit_mle: unsupported distribution/data signature")
    end
end

# MvNormal MLE: `x` is a d×n matrix whose columns are observations.
# μ̂ = row means, Σ̂ = (1/n) Σ (xᵢ - μ̂)(xᵢ - μ̂)ᵀ.
function _fit_mvnormal(x)
    d = size(x, 1)
    n = size(x, 2)
    μ = zeros(d)
    for i in 1:d
        s = 0.0
        for j in 1:n
            s += x[i, j]
        end
        μ[i] = s / n
    end
    Σ = zeros(d, d)
    for a in 1:d
        for b in 1:d
            s = 0.0
            for j in 1:n
                s += (x[a, j] - μ[a]) * (x[b, j] - μ[b])
            end
            Σ[a, b] = s / n
        end
    end
    return MvNormal(μ, Σ)
end

function fit(D, x)
    if D === Beta
        return _fit_beta_moments(suffstats(D, x))
    elseif D === Cauchy
        return fit_mle(D, suffstats(D, x))
    else
        return fit_mle(D, x)
    end
end

fit(D, n::Integer, x) = fit_mle(D, n, x)
