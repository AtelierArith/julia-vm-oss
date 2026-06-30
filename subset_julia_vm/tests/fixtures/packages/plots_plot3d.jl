using Test
using Plots

@testset "Plots: plot(x,y,z) 3D line" begin
    xs = [0.0, 1.0, 2.0]
    ys = [0.0, 1.0, 0.0]
    zs = [0.0, 1.0, 2.0]
    p = plot(xs, ys, zs)
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :path3d
    @test length(s.x) == 3
    @test length(s.y) == 3
    @test length(s.z) == 3
end

true
