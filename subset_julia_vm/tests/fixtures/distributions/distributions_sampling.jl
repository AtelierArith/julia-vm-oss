using Distributions
using Random

# Sampling sanity: with a fixed seed, the empirical mean of a large sample
# should land within a few standard errors of the true mean. Uses the global
# RNG (sampling is global-RNG based in this subset). The distribution is
# constructed inside each helper so `rand(d)` dispatches on a concretely-typed
# local (the compiler cannot dispatch `rand` on an `Any`-typed argument).
#
# N and the tolerances were scaled together to keep the test fast (Issue #7589):
# halving the sample count quadruples nothing — the standard error grows only as
# 1/sqrt(N), so cutting N by 4x doubles the SE. The tolerances below are widened
# by the same factor so every check keeps its original ~5-14 standard-error
# margin while the loop runs 4x fewer iterations.
const N = 5000

function mean_normal()
    Random.seed!(20240620)
    d = Normal(1.0, 2.0)
    s = 0.0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

function mean_uniform()
    Random.seed!(20240620)
    d = Uniform(0.0, 1.0)
    s = 0.0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

function mean_exponential()
    Random.seed!(20240620)
    d = Exponential(3.0)
    s = 0.0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

function mean_gamma()
    Random.seed!(20240620)
    d = Gamma(2.0, 1.5)
    s = 0.0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

function mean_beta()
    Random.seed!(20240620)
    d = Beta(2.0, 3.0)
    s = 0.0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

ok = true
ok = ok && (abs(mean_normal() - 1.0) < 0.16)        # SE ≈ 0.028 at N=5000
ok = ok && (abs(mean_uniform() - 0.5) < 0.04)
ok = ok && (abs(mean_exponential() - 3.0) < 0.30)
ok = ok && (abs(mean_gamma() - 3.0) < 0.30)
ok = ok && (abs(mean_beta() - 0.4) < 0.04)
ok = ok && (rand(Normal(0.0, 1.0)) isa Float64)

ok
