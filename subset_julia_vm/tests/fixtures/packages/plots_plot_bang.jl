using Test
using Plots

@testset "Plots: plot! appends series to current plot" begin
    p = plot(sin)
    p2 = plot!(cos)
    @test length(p.series) == 2
    @test length(p2.series) == 2
    @test length(p2.series[1].x) == 100
    @test length(p2.series[2].x) == 100
    @test isapprox(p2.series[2].y[1], cos(-5.0))
    @test isapprox(p2.series[2].y[end], cos(5.0))
    p2b = plot!(tan)
    @test length(p2b.series) == 3

    ys = [30.0, 40.0]
    p3 = plot([10.0, 20.0])
    p3b = plot!(ys)
    @test length(p3.series) == 2
    @test length(p3b.series) == 2
    @test p3b.series[2].x == [1, 2]
    @test p3b.series[2].y == ys

    xs = [3, 4]
    ys2 = [50.0, 60.0]
    p4 = plot([1, 2], [10.0, 20.0])
    p4b = plot!(xs, ys2)
    @test length(p4.series) == 2
    @test length(p4b.series) == 2
    @test p4b.series[2].x == xs
    @test p4b.series[2].y == ys2
end

true
