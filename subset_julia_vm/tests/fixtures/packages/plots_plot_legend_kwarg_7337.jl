using Test
using Plots

# Issue #7337: 2D `plot`/`scatter` must accept the universal `legend` keyword the
# same way `plot3d` already does. In upstream Plots.jl `legend` is a universal
# plot attribute valid on both 2D and 3D plots; sjulia previously rejected it on
# the 2D path with `MethodError: ... unsupported keyword argument "legend"`
# because the 2D `plot` family lacked a `kwargs...` catch-all (the 3D `plot3d`
# path had it). Display-only kwargs like `legend` are accepted and ignored, like
# the 3D path.
@testset "Plots: plot(x, y; legend=...) accepts legend (Issue #7337)" begin
    plt = plot([1, 2, 3], [1, 4, 9], legend=false)
    @test isa(plt, Plot)
    @test length(plt.series) == 1
    s = plt.series[1]
    @test s.seriestype === :line
    @test length(s.x) == 3
    @test length(s.y) == 3
end

@testset "Plots: 2D plot family accepts legend on all overloads" begin
    @test isa(plot([1, 4, 9], legend=false), Plot)
    @test isa(plot(sin, legend=:topright), Plot)
    @test isa(scatter([1, 2, 3], [1, 4, 9], legend=:bottomleft), Plot)
    @test isa(scatter([1.0, 2.0, 3.0], legend=false), Plot)
end

@testset "Plots: legend threads through plot!/scatter! too" begin
    plot([1, 2, 3], [1, 4, 9], legend=false)
    plt = plot!([1, 2, 3], [9, 4, 1], legend=true)
    @test isa(plt, Plot)
    @test length(plt.series) == 2
    plt2 = scatter!([1, 2, 3], [2, 2, 2], legend=:top)
    @test length(plt2.series) == 3
end

true
