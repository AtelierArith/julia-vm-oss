using Test
using Plots

@testset "Plots: plot(y::Vector)" begin
    ys = [10.0, 20.0, 30.0]
    p = plot(ys)
    @test length(p.series) == 1
    s = p.series[1]
    @test length(s.x) == 3
    @test length(s.y) == 3
end

true
