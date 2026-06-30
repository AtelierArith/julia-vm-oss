using Test
using Plots

@testset "Plots: surface(x,y,z) 3D surface" begin
    xs = [0.0, 1.0, 2.0]
    ys = [0.0, 1.0]
    # z orientation: size(z) == (length(ys), length(xs)) i.e. row=y, col=x
    z = [0.0 1.0 2.0; 3.0 4.0 5.0]
    p = surface(xs, ys, z)
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :surface
    @test length(s.x) == 3
    @test length(s.y) == 2
end

@testset "Plots: surface(x,y,zf) samples z function" begin
    xs = [10.0, 20.0]
    ys = [1.0, 2.0, 3.0]
    p = surface(xs, ys, (x, y) -> y * 100 + x)
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :surface
    @test length(s.x) == 2
    @test length(s.y) == 3
    @test s.z[1, 1] == 110.0
    @test s.z[2, 1] == 210.0
    @test s.z[3, 2] == 320.0
end

true
