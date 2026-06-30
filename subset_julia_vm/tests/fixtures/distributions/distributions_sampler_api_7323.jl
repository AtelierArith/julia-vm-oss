using Distributions
using Random

function explicit_rng_scalar_is_reproducible_7323()
    a = rand(Xoshiro(7), Normal(1.0, 2.0))
    b = rand(Xoshiro(7), Normal(1.0, 2.0))
    c = rand(Xoshiro(8), Normal(1.0, 2.0))
    return a == b && a != c && a isa Float64
end

function explicit_rng_scalar_advances_7323()
    rng = Xoshiro(7)
    d = Normal()
    a = rand(rng, d)
    b = rand(rng, d)
    fresh = Xoshiro(7)
    c = rand(fresh, d)
    e = rand(fresh, d)
    return a == c && b == e && a != b
end

function vector_sampling_uses_requested_length_7323()
    x = rand(Normal(), 5)
    y = rand(Xoshiro(9), Bernoulli(0.25), 6)
    return x isa Vector && y isa Vector && length(x) == 5 && length(y) == 6 &&
           x[1] isa Float64 && y[1] isa Int
end

function array_sampling_uses_requested_dims_7323()
    x = rand(Xoshiro(9), Uniform(2.0, 4.0), 2, 3)
    y = rand(Binomial(4, 0.5), 2, 2)
    return size(x, 1) == 2 && size(x, 2) == 3 &&
           size(y, 1) == 2 && size(y, 2) == 2 &&
           2.0 <= x[1, 1] <= 4.0 && 0 <= y[1, 1] <= 4
end

function rand_bang_fills_existing_array_7323()
    rng = Xoshiro(11)
    d = Poisson(3.0)
    a = zeros(Int64, 4)
    r = rand!(rng, d, a)
    return r === a && length(a) == 4 && a[1] isa Int &&
           a[1] >= 0 && a[2] >= 0 && a[3] >= 0 && a[4] >= 0
end

function sampler_default_returns_distribution_7323()
    d = Gamma(2.0, 3.0)
    s = sampler(d)
    return params(s) == params(d) && mean(s) == mean(d)
end

function all_existing_univariates_have_explicit_rng_7323()
    rng = Xoshiro(13)
    ok = true
    ok = ok && (rand(rng, Normal()) isa Float64)
    ok = ok && (rand(rng, Uniform()) isa Float64)
    ok = ok && (rand(rng, Exponential()) isa Float64)
    ok = ok && (rand(rng, Gamma()) isa Float64)
    ok = ok && (rand(rng, Beta()) isa Float64)
    ok = ok && (rand(rng, Cauchy()) isa Float64)
    ok = ok && (rand(rng, LogNormal()) isa Float64)
    ok = ok && (rand(rng, Weibull()) isa Float64)
    ok = ok && (rand(rng, Bernoulli()) isa Int)
    ok = ok && (rand(rng, Binomial(4, 0.5)) isa Int)
    ok = ok && (rand(rng, Poisson()) isa Int)
    ok = ok && (rand(rng, Geometric()) isa Int)
    ok = ok && (rand(rng, DiscreteUniform(1, 3)) isa Int)
    ok = ok && (rand(rng, Categorical([0.2, 0.3, 0.5])) isa Int)
    return ok
end

ok = true
ok = ok && explicit_rng_scalar_is_reproducible_7323()
ok = ok && explicit_rng_scalar_advances_7323()
ok = ok && vector_sampling_uses_requested_length_7323()
ok = ok && array_sampling_uses_requested_dims_7323()
ok = ok && rand_bang_fills_existing_array_7323()
ok = ok && sampler_default_returns_distribution_7323()
ok = ok && all_existing_univariates_have_explicit_rng_7323()

ok
