using Test
using Distributions
using StatsPlots

# Discrete distributions become integer-support :bar columns of the pmf, sampled
# over the quantile range (Issue #7262). Upstream uses :sticks; the bundled Plots
# backend renders :bar, the closest faithful column form.
@testset "StatsPlots: discrete distributions" begin
    # Poisson(3): support 0,1,2,…; the central 99.98% lies in roughly 0:11.
    dp = Poisson(3.0)
    pp = plot(dp)
    @test pp isa Plot
    @test length(pp.series) == 1
    sp = pp.series[1]
    @test sp.seriestype === :bar

    # Bars sit on consecutive integers from floor(quantile(d,0.0001)) up.
    klo = floor(quantile(dp, 0.0001))
    khi = ceil(quantile(dp, 0.9999))
    @test isapprox(sp.x[1], klo; atol=1e-9)
    @test isapprox(sp.x[end], khi; atol=1e-9)
    @test length(sp.x) == Int(khi - klo) + 1
    # Consecutive integer spacing.
    @test isapprox(sp.x[2] - sp.x[1], 1.0; atol=1e-9)

    # Each bar height is exactly the pmf at that integer.
    for i in 1:length(sp.x)
        @test isapprox(sp.y[i], pdf(dp, sp.x[i]); atol=1e-12)
    end
    # Probabilities are non-negative and sum to (almost) 1 over the central range.
    @test all(y -> y >= 0.0, sp.y)
    @test sum(sp.y) > 0.99

    # Mode of Poisson(3) is at k = 2 or 3 (both pmf ≈ 0.224); the tallest bar
    # should be one of them.
    peak_idx = 1
    for i in 2:length(sp.y)
        if sp.y[i] > sp.y[peak_idx]
            peak_idx = i
        end
    end
    @test sp.x[peak_idx] == 2.0 || sp.x[peak_idx] == 3.0

    # Bernoulli(0.3): support {0, 1}, exactly two bars.
    db = Bernoulli(0.3)
    pb = plot(db)
    sb = pb.series[1]
    @test sb.seriestype === :bar
    @test length(sb.x) == 2
    @test isapprox(sb.x[1], 0.0; atol=1e-9)
    @test isapprox(sb.x[2], 1.0; atol=1e-9)
    @test isapprox(sb.y[1], 0.7; atol=1e-9)
    @test isapprox(sb.y[2], 0.3; atol=1e-9)
end

true
