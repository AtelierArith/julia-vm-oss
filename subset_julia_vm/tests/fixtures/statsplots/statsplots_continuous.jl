using Test
using Distributions
using StatsPlots

# Continuous distributions other than Normal also become a pdf :line plot over
# their quantile range (Issue #7262).
@testset "StatsPlots: continuous distributions" begin
    # Uniform(0, 1): pdf is the constant 1.0 on [0, 1]; the quantile range is
    # [quantile(d, 0.0001), quantile(d, 0.9999)] = [0.0001, 0.9999].
    du = Uniform(0.0, 1.0)
    pu = plot(du)
    @test pu isa Plot
    su = pu.series[1]
    @test su.seriestype === :line
    @test length(su.x) == 100
    @test isapprox(su.x[1], quantile(du, 0.0001); atol=1e-9)
    @test isapprox(su.x[end], quantile(du, 0.9999); atol=1e-9)
    # Interior pdf values are all 1.0 (density of the standard uniform).
    @test isapprox(su.y[50], 1.0; atol=1e-9)
    @test isapprox(su.y[1], pdf(du, su.x[1]); atol=1e-12)

    # Exponential(1.0): pdf decreases monotonically from x ≈ 0.
    de = Exponential(1.0)
    pe = plot(de)
    se = pe.series[1]
    @test se.seriestype === :line
    @test length(se.x) == 100
    @test isapprox(se.x[1], quantile(de, 0.0001); atol=1e-9)
    @test isapprox(se.x[end], quantile(de, 0.9999); atol=1e-9)
    @test se.x[1] >= 0.0
    @test se.y[1] > se.y[end]            # monotone decreasing density
    @test isapprox(se.y[10], pdf(de, se.x[10]); atol=1e-12)
end

true
