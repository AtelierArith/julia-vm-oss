using Distributions
using Random

tol = 1e-6

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

function samples_in_support(d, xs)
    lo = minimum(d)
    hi = maximum(d)
    for x in xs
        if !(lo <= x <= hi)
            return false
        end
    end
    return true
end

function laplace_api_7328()
    d = Laplace(1.0, 2.0)
    xs = rand(Xoshiro(123), d, 2000)
    m, v = sample_stats(xs)
    return params(d) == (1.0, 2.0) &&
           close(location(d), 1.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 1.0) &&
           close(var(d), 8.0) &&
           close(median(d), 1.0) &&
           close(mode(d), 1.0) &&
           close(entropy(d), 2.386294361119891) &&
           minimum(d) == -Inf &&
           maximum(d) == Inf &&
           close(pdf(d, 0.0), 0.15163266492815836) &&
           close(logpdf(d, 0.0), -1.8862943611198906) &&
           close(cdf(d, 0.0), 0.3032653298563167) &&
           close(quantile(d, 0.75), 2.386294361119891) &&
           close_atol(m, mean(d), 0.25) &&
           close_atol(v, var(d), 1.3)
end

function logistic_api_7328()
    d = Logistic(1.0, 2.0)
    xs = rand(Xoshiro(123), d, 2000)
    m, v = sample_stats(xs)
    return params(d) == (1.0, 2.0) &&
           close(location(d), 1.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 1.0) &&
           close(var(d), 13.159472534785811) &&
           close(median(d), 1.0) &&
           close(mode(d), 1.0) &&
           close(entropy(d), 2.6931471805599454) &&
           close(pdf(d, 1.0), 0.125) &&
           close(logpdf(d, 1.0), -2.0794415416798357) &&
           close(cdf(d, 1.0), 0.5) &&
           close(quantile(d, 0.75), 3.1972245773362196) &&
           close_atol(m, mean(d), 0.35) &&
           close_atol(v, var(d), 2.2)
end

function rayleigh_api_7328()
    d = Rayleigh(2.0)
    xs = rand(Xoshiro(123), d, 2000)
    m, v = sample_stats(xs)
    return params(d) == (2.0,) &&
           close(scale(d), 2.0) &&
           close(mean(d), 2.5066282746310007) &&
           close(var(d), 1.7168146928204138) &&
           close(median(d), 2.3548200450309493) &&
           close(mode(d), 2.0) &&
           close(entropy(d), 1.635181422730739) &&
           minimum(d) == 0.0 &&
           maximum(d) == Inf &&
           close(pdf(d, 2.0), 0.3032653298563167) &&
           close(logpdf(d, 2.0), -1.1931471805599454) &&
           close(cdf(d, 2.0), 0.3934693402873666) &&
           close(quantile(d, 0.6321205588285577), 2.8284271247461903) &&
           samples_in_support(d, xs) &&
           close_atol(m, mean(d), 0.12) &&
           close_atol(v, var(d), 0.2)
end

function pareto_api_7328()
    d = Pareto(5.0, 2.0)
    xs = rand(Xoshiro(123), d, 2000)
    m, v = sample_stats(xs)
    return params(d) == (5.0, 2.0) &&
           close(shape(d), 5.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 2.5) &&
           close(var(d), 0.4166666666666667) &&
           close(median(d), 2.29739670999407) &&
           close(mode(d), 2.0) &&
           close(entropy(d), 0.28370926812584507) &&
           minimum(d) == 2.0 &&
           maximum(d) == Inf &&
           close(pdf(d, 3.0), 0.21947873799725642) &&
           close(logpdf(d, 3.0), -1.5164999167748316) &&
           close(cdf(d, 3.0), 0.8683127572016461) &&
           close(quantile(d, 0.75), 2.639015821545789) &&
           samples_in_support(d, xs) &&
           close_atol(m, mean(d), 0.08) &&
           close_atol(v, var(d), 0.15)
end

function gumbel_api_7328()
    d = Gumbel(1.0, 2.0)
    xs = rand(Xoshiro(123), d, 2000)
    m, v = sample_stats(xs)
    return params(d) == (1.0, 2.0) &&
           close(location(d), 1.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 2.1544313298030655) &&
           close(var(d), 6.579736267392906) &&
           close(median(d), 1.7330258411633288) &&
           close(mode(d), 1.0) &&
           close(entropy(d), 2.270362845461478) &&
           close(pdf(d, 1.0), 0.18393972058572117) &&
           close(logpdf(d, 1.0), -1.6931471805599454) &&
           close(cdf(d, 1.0), 0.36787944117144233) &&
           close(quantile(d, 0.5), 1.7330258411633288) &&
           close_atol(m, mean(d), 0.22) &&
           close_atol(v, var(d), 1.0)
end

function frechet_api_7328()
    d = Frechet(5.0, 2.0)
    xs = rand(Xoshiro(123), d, 2000)
    m, v = sample_stats(xs)
    return params(d) == (5.0, 2.0) &&
           close(shape(d), 5.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 2.3284594274506065) &&
           close(var(d), 0.5350456899676619) &&
           close(median(d), 2.15211217027801) &&
           close(mode(d), 1.9283850080052545) &&
           close(entropy(d), 0.7763680660076845) &&
           minimum(d) == 0.0 &&
           maximum(d) == Inf &&
           close(pdf(d, 2.0), 0.9196986029286058) &&
           close(logpdf(d, 2.0), -0.0837092681258449) &&
           close(cdf(d, 2.0), 0.36787944117144233) &&
           close(quantile(d, 0.5), 2.15211217027801) &&
           samples_in_support(d, xs) &&
           close_atol(m, mean(d), 0.12) &&
           close_atol(v, var(d), 0.25)
end

function levy_api_7328()
    d = Levy(0.0, 1.0)
    xs = rand(Xoshiro(123), d, 64)
    return params(d) == (0.0, 1.0) &&
           location(d) == 0.0 &&
           mean(d) == Inf &&
           var(d) == Inf &&
           close(median(d), 2.198109338317732) &&
           close(mode(d), 0.3333333333333333) &&
           close(entropy(d), 3.3244828013968895) &&
           minimum(d) == 0.0 &&
           maximum(d) == Inf &&
           close(pdf(d, 1.0), 0.24197072451914334) &&
           close(logpdf(d, 1.0), -1.4189385332046727) &&
           close(cdf(d, 1.0), 0.31731050786291404) &&
           close(quantile(d, 0.5), 2.198109338317732) &&
           samples_in_support(d, xs)
end

function dims_sampling_api_7328()
    a = rand(Xoshiro(321), Rayleigh(2.0), 2, 3)
    return size(a) == (2, 3) && samples_in_support(Rayleigh(2.0), a)
end

ok = true
ok = ok && laplace_api_7328()
ok = ok && logistic_api_7328()
ok = ok && rayleigh_api_7328()
ok = ok && pareto_api_7328()
ok = ok && gumbel_api_7328()
ok = ok && frechet_api_7328()
ok = ok && levy_api_7328()
ok = ok && dims_sampling_api_7328()

ok
