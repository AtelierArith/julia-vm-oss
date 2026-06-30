using Test
# The headline Issue #7262 form: a single comma-separated `using` line bringing in
# both packages, then `plot(Normal(0, 1))`. (Comma-form `using A, B` is now lowered
# to one import per module — it previously failed with a bogus "module 'A, B' not
# found".)
using Distributions, StatsPlots

@testset "StatsPlots: using Distributions, StatsPlots" begin
    p = plot(Normal(0, 1))
    @test p isa Plot
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :line
    @test length(s.x) == 100
    # Peak of the standard normal pdf at the center.
    @test isapprox(s.y[50], 0.3989422804; atol=1e-3)
end

true
