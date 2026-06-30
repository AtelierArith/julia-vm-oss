using Test
using Plots

@testset "Plots: xlims!/ylims! setter and xlims/ylims getter (Issue #7850)" begin
    p = plot([1.0, 2.0, 3.0], [4.0, 9.0, 16.0])
    @test p.xlims === nothing
    @test p.ylims === nothing

    # xlims! on explicit plot
    p2 = xlims!(p, 0.0, 4.0)
    @test p2.xlims == (0.0, 4.0)
    @test p2.ylims === nothing

    # ylims! on explicit plot
    p3 = ylims!(p2, -1.0, 20.0)
    @test p3.ylims == (-1.0, 20.0)
    @test p3.xlims == (0.0, 4.0)

    # Tuple form
    p4 = xlims!(p, (0.5, 3.5))
    @test p4.xlims == (0.5, 3.5)
    p5 = ylims!(p, (0.0, 18.0))
    @test p5.ylims == (0.0, 18.0)

    # No-argument form applies to current()
    p6 = plot([10.0, 20.0], [1.0, 2.0])
    p7 = xlims!(5.0, 25.0)
    @test p7.xlims == (5.0, 25.0)
    p8 = ylims!(0.0, 3.0)
    @test p8.ylims == (0.0, 3.0)

    # xlims getter: no explicit range → compute from data
    p9 = plot([2.0, 4.0, 6.0], [1.0, 3.0, 5.0])
    xl = xlims(p9)
    @test xl[1] ≈ 2.0
    @test xl[2] ≈ 6.0
    yl = ylims(p9)
    @test yl[1] ≈ 1.0
    @test yl[2] ≈ 5.0

    # xlims getter: explicit range → return it directly
    p10 = xlims!(p9, 0.0, 10.0)
    xl2 = xlims(p10)
    @test xl2 == (0.0, 10.0)

    # No-arg getter uses current()
    p11 = plot([1.0, 2.0], [0.0, 1.0])
    xlims!(1.0, 3.0)
    xl3 = xlims()
    @test xl3 == (1.0, 3.0)
end

true
