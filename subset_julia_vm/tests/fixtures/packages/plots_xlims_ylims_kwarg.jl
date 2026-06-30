using Test
using Plots

@testset "Plots: xlims/ylims keyword arguments passed to plot() (Issue #8108)" begin
    # xlims kwarg is stored in the returned Plot
    p = plot([1.0, 2.0, 3.0], [4.0, 9.0, 16.0]; xlims=(0.0, 5.0))
    @test p.xlims == (0.0, 5.0)
    @test p.ylims === nothing

    # ylims kwarg is stored
    p2 = plot([1.0, 2.0], [0.0, 1.0]; ylims=(-1.0, 2.0))
    @test p2.xlims === nothing
    @test p2.ylims == (-1.0, 2.0)

    # both xlims and ylims together
    p3 = plot([1.0, 2.0], [3.0, 4.0]; xlims=(-1.0, 3.0), ylims=(2.0, 5.0))
    @test p3.xlims == (-1.0, 3.0)
    @test p3.ylims == (2.0, 5.0)

    # xlims kwarg propagated from plot(y::Vector; ...)
    p4 = plot([10.0, 20.0]; xlims=(5.0, 25.0))
    @test p4.xlims == (5.0, 25.0)

    # scatter with xlims kwarg
    p5 = scatter([1.0, 2.0], [3.0, 4.0]; xlims=(0.0, 3.0), ylims=(2.0, 6.0))
    @test p5.xlims == (0.0, 3.0)
    @test p5.ylims == (2.0, 6.0)

    # plot! inherits xlims from initial plot() call
    p6 = plot([1.0, 2.0], [1.0, 2.0]; xlims=(-2.0, 4.0), ylims=(-2.0, 4.0))
    p7 = plot!([3.0, 4.0], [3.0, 4.0])
    @test p7.xlims == (-2.0, 4.0)
    @test p7.ylims == (-2.0, 4.0)
end

true
