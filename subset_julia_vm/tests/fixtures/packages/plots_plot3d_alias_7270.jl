using Test
using Plots

# Issue #7270: `plot3d` is an exported alias for a 3D path plot.
#   plot3d(x, y, z; kw...)  ==  plot(x, y, z; seriestype = :path3d, kw...)
#   plot3d(n::Integer; kw...) initializes a Plot with n EMPTY series (for push!).
@testset "Plots: plot3d(x,y,z) alias" begin
    plt = plot3d([1.0, 2.0], [1.0, 2.0], [1.0, 2.0])
    @test isa(plt, Plot)
    @test length(plt.series) == 1
    s = plt.series[1]
    @test s.seriestype === :path3d
    @test length(s.x) == 2
    @test length(s.y) == 2
    @test length(s.z) == 2
end

@testset "Plots: plot3d(n::Integer) seeds n empty series" begin
    plt = plot3d(1)
    @test isa(plt, Plot)
    @test length(plt.series) == 1
    s = plt.series[1]
    @test s.seriestype === :path3d
    @test length(s.x) == 0
    @test length(s.y) == 0
    @test length(s.z) == 0

    plt3 = plot3d(3)
    @test length(plt3.series) == 3
    @test plt3.series[2].seriestype === :path3d
    @test length(plt3.series[3].y) == 0
end

@testset "Plots: plot3d(n) accepts the Lorenz kwargs" begin
    plt = plot3d(1, xlim=(-30, 30), ylim=(-30, 30), zlim=(0, 60), title="Lorenz", legend=false, marker=2)
    @test isa(plt, Plot)
    @test length(plt.series) == 1
    @test plt.title == "Lorenz"
end

true
