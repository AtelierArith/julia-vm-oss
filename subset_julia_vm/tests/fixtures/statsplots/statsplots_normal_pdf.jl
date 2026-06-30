using Test
using Distributions
using StatsPlots

# `plot(Normal(0, 1))` draws the pdf bell curve over the central 99.98% of the
# mass (Issue #7262). The expected x-range comes from the bundled Normal quantile
# (Acklam approximation): quantile(Normal(0,1), 0.0001) ≈ -3.71901648,
# quantile(Normal(0,1), 0.9999) ≈ 3.71901648. (Upstream Distributions is not
# installed in this environment, so values are pinned to the bundled
# implementation, which agrees with the exact erfinv quantile to < 1e-8.)
@testset "StatsPlots: Normal pdf line plot" begin
    d = Normal(0.0, 1.0)
    p = plot(d)

    # Returns a Plot with a single :line series.
    @test p isa Plot
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :line

    # 100 sample points spanning the quantile range.
    @test length(s.x) == 100
    @test length(s.y) == 100

    # Endpoints match the distribution quantiles.
    @test isapprox(s.x[1], quantile(d, 0.0001); atol=1e-9)
    @test isapprox(s.x[end], quantile(d, 0.9999); atol=1e-9)
    @test isapprox(s.x[1], -3.7190164821; atol=1e-6)
    @test isapprox(s.x[end], 3.7190164821; atol=1e-6)

    # y values are exactly the pdf evaluated at each x.
    @test isapprox(s.y[1], pdf(d, s.x[1]); atol=1e-12)
    @test isapprox(s.y[end], pdf(d, s.x[end]); atol=1e-12)

    # Symmetric bell curve: unimodal with the peak at the center (x ≈ 0).
    mid = 50
    @test isapprox(s.x[mid], 0.0; atol=0.1)
    peak = s.y[mid]
    @test isapprox(peak, 0.3989422804; atol=1e-3)
    # Strictly increasing up to the peak, strictly decreasing after.
    @test s.y[1] < s.y[25] < s.y[mid]
    @test s.y[mid] > s.y[75] > s.y[end]
    # Tails are small.
    @test s.y[1] < 1e-3
    @test s.y[end] < 1e-3
end

true
