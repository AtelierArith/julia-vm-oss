using Test
using Plots

@testset "Plots: scatter(x,y,z) 3D scatter" begin
    xs = [1.0, 2.0, 3.0]
    ys = [4.0, 5.0, 6.0]
    zs = [7.0, 8.0, 9.0]
    p = scatter(xs, ys, zs)
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :scatter3d
    @test length(s.x) == 3
    @test length(s.y) == 3
    @test length(s.z) == 3
end

true
