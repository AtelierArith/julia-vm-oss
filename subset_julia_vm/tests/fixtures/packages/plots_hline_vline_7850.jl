using Test
using Plots

@testset "Plots: hline/vline non-bang (Issue #7850)" begin
    # hline(ys) creates a new plot with hlines set, no data series
    p = hline([1.0, 2.0])
    @test length(p.hlines) == 2
    @test p.hlines[1] ≈ 1.0
    @test p.hlines[2] ≈ 2.0
    @test length(p.vlines) == 0

    # hline(y) single value
    p2 = hline(0.5)
    @test length(p2.hlines) == 1
    @test p2.hlines[1] ≈ 0.5

    # vline(xs) creates a new plot with vlines set
    p3 = vline([0.3, 0.7])
    @test length(p3.vlines) == 2
    @test p3.vlines[1] ≈ 0.3
    @test p3.vlines[2] ≈ 0.7
    @test length(p3.hlines) == 0

    # vline(x) single value
    p4 = vline(1.5)
    @test length(p4.vlines) == 1
    @test p4.vlines[1] ≈ 1.5

    # hlines/vlines default to empty on regular plots
    p5 = plot([1.0, 2.0], [3.0, 4.0])
    @test length(p5.hlines) == 0
    @test length(p5.vlines) == 0
end

true
