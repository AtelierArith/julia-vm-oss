using Distributions
using Random

tol = 1e-6

# Workaround: omitted keyword defaults that reference globals can evaluate as 0
# in sjulia (Issue #7774). Keep tolerance helpers positional-only here.
close(a, b) = abs(a - b) < tol
close_atol(a, b, atol) = abs(a - b) < atol

function sample_stats(xs)
    s = 0.0
    for x in xs
        s += x
    end
    m = s / length(xs)
    v = 0.0
    for x in xs
        v += (x - m)^2
    end
    return (m, v / length(xs))
end

function tdist_api_7327()
    d = TDist(5.0)
    qs = (quantile(d, 0.25), quantile(d, 0.5), quantile(d, 0.75))
    m, v = sample_stats(rand(Xoshiro(123), d, 2000))
    return params(d) == (5.0,) &&
           close(mean(d), 0.0) &&
           close(var(d), 1.6666666666666667) &&
           close(mode(d), 0.0) &&
           minimum(d) == -Inf &&
           maximum(d) == Inf &&
           close(pdf(d, 1.0), 0.21967979735098056) &&
           close(logpdf(d, 1.0), -1.515584259436588) &&
           close(cdf(d, 1.0), 0.8183912661754387) &&
           close_atol(qs[1], -0.7266868438004228, 1e-5) &&
           close(qs[2], 0.0) &&
           close_atol(qs[3], 0.7266868438004228, 1e-5) &&
           abs(m) < 0.12 &&
           close_atol(v, var(d), 0.35)
end

function chisq_api_7327()
    d = Chisq(4.0)
    qs = (quantile(d, 0.25), quantile(d, 0.5), quantile(d, 0.75))
    m, v = sample_stats(rand(Xoshiro(123), d, 2000))
    return params(d) == (4.0,) &&
           close(mean(d), 4.0) &&
           close(var(d), 8.0) &&
           close(mode(d), 2.0) &&
           minimum(d) == 0.0 &&
           maximum(d) == Inf &&
           close(pdf(d, 1.0), 0.15163266492815833) &&
           close(logpdf(d, 1.0), -1.8862943611198908) &&
           close(cdf(d, 1.0), 0.09020401043104986) &&
           close_atol(qs[1], 1.9225575262295542, 1e-5) &&
           close_atol(qs[2], 3.3566939800333224, 1e-5) &&
           close_atol(qs[3], 5.385269057779392, 1e-5) &&
           close_atol(m, mean(d), 0.25) &&
           close_atol(v, var(d), 0.8)
end

function fdist_api_7327()
    d = FDist(5.0, 10.0)
    qs = (quantile(d, 0.25), quantile(d, 0.5), quantile(d, 0.75))
    m, v = sample_stats(rand(Xoshiro(123), d, 2000))
    return params(d) == (5.0, 10.0) &&
           close(mean(d), 1.25) &&
           close(var(d), 1.3541666666666667) &&
           close(mode(d), 0.5) &&
           minimum(d) == 0.0 &&
           maximum(d) == Inf &&
           close(pdf(d, 1.0), 0.49547978348663907) &&
           close(logpdf(d, 1.0), -0.7022287262732272) &&
           close(cdf(d, 1.0), 0.5348805734621996) &&
           close_atol(qs[1], 0.5291416855678216, 1e-5) &&
           close_atol(qs[2], 0.9319331608510479, 1e-5) &&
           close_atol(qs[3], 1.5853232593846158, 1e-5) &&
           close_atol(m, mean(d), 0.2) &&
           close_atol(v, var(d), 0.45)
end

ok = true
ok = ok && tdist_api_7327()
ok = ok && chisq_api_7327()
ok = ok && fdist_api_7327()

ok
