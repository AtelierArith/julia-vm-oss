using Test
using Plots

@testset "Plots: bar and bar! (Issue #6358)" begin
    p = bar([1, 2, 3], [4, 5, 6])
    @test length(p.series) == 1
    @test p.series[1].seriestype === :bar
    @test p.series[1].x == [1, 2, 3]
    @test p.series[1].y == [4, 5, 6]

    pv = bar([4, 5, 6])
    @test pv.series[1].seriestype === :bar
    @test pv.series[1].x == [1, 2, 3]
    @test pv.series[1].y == [4, 5, 6]

    pp = bar([(1, 4), (2, 5), (3, 6)])
    @test pp.series[1].seriestype === :bar
    @test pp.series[1].x == [1, 2, 3]
    @test pp.series[1].y == [4, 5, 6]

    pk = bar([1, 2, 3], [4, 5, 6], fillcolor=[:red, :green, :blue], fillalpha=[0.2, 0.4, 0.6])
    @test pk.series[1].seriestype === :bar
    @test pk.series[1].x == [1, 2, 3]
    @test pk.series[1].y == [4, 5, 6]

    p2 = plot([0, 1], [0, 1])
    p3 = bar!([1, 2, 3], [6, 5, 4])
    @test length(p2.series) == 2
    @test length(p3.series) == 2
    @test p3.series[1].seriestype === :line
    @test p3.series[2].seriestype === :bar
    @test p3.series[2].x == [1, 2, 3]
    @test p3.series[2].y == [6, 5, 4]
end

true
