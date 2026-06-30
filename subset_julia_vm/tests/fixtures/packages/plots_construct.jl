using Test
using Plots

@testset "Plots: plot(x, y) construction" begin
    xs = [1, 2, 3, 4, 5]
    ys = [1.0, 2.0, 3.0, 4.0, 5.0]
    p = plot(xs, ys)
    @test length(p.series) == 1
    s = p.series[1]
    @test length(s.y) == 5
    @test s.label === nothing
end

true
