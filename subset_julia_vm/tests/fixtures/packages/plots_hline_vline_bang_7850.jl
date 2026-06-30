using Test
using Plots

@testset "Plots: hline!/vline! (Issue #7850)" begin
    p = plot([1.0, 2.0, 3.0], [1.0, 2.0, 3.0])
    @test length(p.hlines) == 0
    @test length(p.vlines) == 0

    # Single value
    p2 = hline!(p, 1.5)
    @test length(p2.hlines) == 1
    @test p2.hlines[1] ≈ 1.5
    @test length(p2.vlines) == 0

    # Vector form accumulates
    p3 = hline!(p2, [0.5, 2.5])
    @test length(p3.hlines) == 3
    @test p3.hlines[1] ≈ 1.5
    @test p3.hlines[2] ≈ 0.5
    @test p3.hlines[3] ≈ 2.5

    # vline! single value
    p4 = vline!(p3, 2.0)
    @test length(p4.vlines) == 1
    @test p4.vlines[1] ≈ 2.0

    # vline! vector form
    p5 = vline!(p4, [0.5, 1.5])
    @test length(p5.vlines) == 3
    @test p5.vlines[2] ≈ 0.5
    @test p5.vlines[3] ≈ 1.5

    # No-argument forms use current()
    p6 = plot([0.0, 1.0], [0.0, 1.0])
    hline!(0.5)
    @test current().hlines[1] ≈ 0.5
    vline!(0.3)
    @test current().vlines[1] ≈ 0.3

    # plot! carries forward existing hlines/vlines
    p7 = hline!(p2, [3.0])
    p8 = plot!([4.0, 5.0], [4.0, 5.0])
    @test length(p8.hlines) == 2  # 1.5 and 3.0
end

true
