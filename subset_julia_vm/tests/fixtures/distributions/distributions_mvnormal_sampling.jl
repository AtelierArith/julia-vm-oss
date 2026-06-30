using Distributions
using Random
using Test

# Seeded global-RNG sampling sanity for MvNormal: the empirical mean and
# covariance of a large sample should be close to μ and Σ.
#
# The sample count and tolerances are scaled together to keep the test fast
# (Issue #7589): the sampling error shrinks only as 1/sqrt(n), so cutting n by
# 4x doubles it. The tolerances below are widened by the same factor to preserve
# the original margin.

function mvnormal_stats(n)
    Random.seed!(20240620)
    d = MvNormal([1.0, -2.0], [2.0 0.5; 0.5 1.0])
    sx = 0.0
    sy = 0.0
    sxx = 0.0
    syy = 0.0
    sxy = 0.0
    for _ in 1:n
        v = rand(d)
        x = v[1]
        y = v[2]
        sx += x
        sy += y
        sxx += x * x
        syy += y * y
        sxy += x * y
    end
    mx = sx / n
    my = sy / n
    cxx = sxx / n - mx * mx
    cyy = syy / n - my * my
    cxy = sxy / n - mx * my
    return (mx, my, cxx, cyy, cxy)
end

function mvnormal_explicit_rng_shape_7756()
    d = MvNormal([1.0, 2.0], [1.0 0.0; 0.0 1.0])
    x = rand(Xoshiro(17), d)
    return length(x) == 2 && x isa Vector && x[1] isa Float64 && x[2] isa Float64
end

function mvnormal_explicit_rng_reproducible_7756()
    d = MvNormal([1.0, 2.0], [1.0 0.0; 0.0 1.0])
    a = rand(Xoshiro(17), d)
    b = rand(Xoshiro(17), d)
    c = rand(Xoshiro(18), d)
    return a == b && (a[1] != c[1] || a[2] != c[2])
end

function mvnormal_explicit_rng_var_advances_7756()
    d = MvNormal([1.0, 2.0], [1.0 0.0; 0.0 1.0])
    rng = Xoshiro(17)
    a = rand(rng, d)
    b = rand(rng, d)
    fresh = Xoshiro(17)
    c = rand(fresh, d)
    e = rand(fresh, d)
    return a == c && b == e && (a[1] != b[1] || a[2] != b[2])
end

n = 10000
mx, my, cxx, cyy, cxy = mvnormal_stats(n)

@testset "Distributions MvNormal sampling" begin
    @test abs(mx - 1.0) < 0.1
    @test abs(my + 2.0) < 0.1
    @test abs(cxx - 2.0) < 0.2
    @test abs(cyy - 1.0) < 0.2
    @test abs(cxy - 0.5) < 0.2
    # a single draw is a length-2 vector
    @test length(rand(MvNormal([0.0, 0.0], [1.0 0.0; 0.0 1.0]))) == 2
    @test mvnormal_explicit_rng_shape_7756()
    @test mvnormal_explicit_rng_reproducible_7756()
    @test mvnormal_explicit_rng_var_advances_7756()
end

true
