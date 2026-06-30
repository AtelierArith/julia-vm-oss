# Plots: x plus f::Function overloads from included package API (Issues #4448/#4452)

using Test
using Plots

@testset "Plots: plot/scatter with x and f::Function (Issues #4448/#4452)" begin
    xs = [0.0, 1.0, 2.0]
    expected_cos = [cos(0.0), cos(1.0), cos(2.0)]
    expected_sin = [sin(0.0), sin(1.0), sin(2.0)]

    p = plot(xs, cos)
    @test p.series[1].x == xs
    @test p.series[1].y == expected_cos
    @test typeof(p.series[1].y) == Vector{Float64}
    @test p.series[1].seriestype === :line

    p2 = plot!(xs, sin)
    @test length(p2.series) == 2
    @test p2.series[2].x == xs
    @test p2.series[2].y == expected_sin
    @test p2.series[2].seriestype === :line

    ps = scatter(xs, cos)
    @test length(ps.series) == 1
    @test ps.series[1].x == xs
    @test ps.series[1].y == expected_cos
    @test ps.series[1].seriestype === :scatter

    ps2 = scatter!(xs, sin)
    @test length(ps2.series) == 2
    @test ps2.series[2].x == xs
    @test ps2.series[2].y == expected_sin
    @test ps2.series[2].seriestype === :scatter

    rxs = -0.1:0.01:1
    collected_rxs = collect(rxs)
    pr = plot(rxs, cos)
    @test pr.series[1].x == collected_rxs
    @test pr.series[1].y == map(cos, collected_rxs)
    @test typeof(pr.series[1].y) == Vector{Float64}

    sr = scatter!(rxs, sin)
    @test sr.series[2].x == collected_rxs
    @test sr.series[2].y == map(sin, collected_rxs)
    @test typeof(sr.series[2].y) == Vector{Float64}
    @test sr.series[2].seriestype === :scatter

    swapped = plot(cos, rxs)
    @test swapped.series[1].x == collected_rxs
    @test swapped.series[1].y == map(cos, collected_rxs)
    @test typeof(swapped.series[1].y) == Vector{Float64}

    swapped_bang = plot!(sin, rxs)
    @test swapped_bang.series[2].x == collected_rxs
    @test swapped_bang.series[2].y == map(sin, collected_rxs)
    @test swapped_bang.series[2].seriestype === :line

    swapped_scatter = scatter(cos, rxs)
    @test swapped_scatter.series[1].x == collected_rxs
    @test swapped_scatter.series[1].y == map(cos, collected_rxs)
    @test swapped_scatter.series[1].seriestype === :scatter

    swapped_scatter_bang = scatter!(sin, rxs)
    @test swapped_scatter_bang.series[2].x == collected_rxs
    @test swapped_scatter_bang.series[2].y == map(sin, collected_rxs)
    @test swapped_scatter_bang.series[2].seriestype === :scatter
end

true
