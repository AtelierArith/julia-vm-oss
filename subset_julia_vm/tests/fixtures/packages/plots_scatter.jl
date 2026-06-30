using Test
using Plots

@testset "Plots: scatter" begin
    # scatter(x, y) tags Series with seriestype = :scatter.
    p = scatter([1, 2, 3], [4.0, 5.0, 6.0])
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :scatter
    @test length(s.x) == 3
    @test length(s.y) == 3

    # plot(...) still defaults to :line.
    p2 = plot([1, 2, 3], [4.0, 5.0, 6.0])
    @test p2.series[1].seriestype === :line

    # scatter(f::Function) shares the (-5, 5) default with plot.
    pf = scatter(sin)
    sf = pf.series[1]
    @test sf.seriestype === :scatter
    @test length(sf.x) == 100
    @test isapprox(sf.x[1], -5.0)
    @test isapprox(sf.x[end], 5.0)

    # scatter(y::Vector) uses 1:length(y) for x.
    pv = scatter([10.0, 20.0, 30.0])
    sv = pv.series[1]
    @test sv.seriestype === :scatter
    @test length(sv.x) == 3
end

true
