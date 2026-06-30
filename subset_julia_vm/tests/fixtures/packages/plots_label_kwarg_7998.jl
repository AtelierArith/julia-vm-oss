using Test
using Plots

# Issue #7998: `label` keyword must be captured on Series values so the Plotly
# artifact pipeline can emit trace names and enable the legend.
@testset "Plots: plot/plot! label keyword is stored on Series (Issue #7998)" begin
    p = plot([1, 2, 3], [1, 4, 9], label="quadratic")
    @test isa(p, Plot)
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :line
    @test s.label == "quadratic"

    p2 = plot!([1, 2, 3], [9, 4, 1], label="cubic")
    @test isa(p2, Plot)
    @test length(p2.series) == 2
    @test p2.series[2].label == "cubic"
end

@testset "Plots: label keyword accepted across 2D constructors" begin
    @test plot([1, 2], [1, 2], label="line").series[1].label == "line"
    @test scatter([1, 2], [1, 2], label="pts").series[1].label == "pts"
    @test bar([1, 2], [1, 2], label="bars").series[1].label == "bars"
    @test plot(sin, label="sin").series[1].label == "sin"
    @test plot([1, 2], label="y").series[1].label == "y"
end

true
