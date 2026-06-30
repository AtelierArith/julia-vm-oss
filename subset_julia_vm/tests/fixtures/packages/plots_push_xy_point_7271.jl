using Test
using Plots

# Issue #7271: push!(plt, x, y) and push!(plt, x, y, z) append a single point to
# the first series, mirroring upstream `extend_series!(series, xi, yi[, zi])`.
# `push!(plt, x, y)` appends x to series.x and y to series.y explicitly (unlike the
# `push!(plt, i, y)` auto-x form). This requires the series x/y/z to be mutable.
@testset "Plots: push!(plt, x, y) appends a 2D point to series 1" begin
    plt = plot([1.0, 2.0], [10.0, 20.0])
    @test length(plt.series[1].x) == 2
    push!(plt, 3.0, 30.0)
    @test length(plt.series[1].x) == 3
    @test length(plt.series[1].y) == 3
    @test plt.series[1].x[3] == 3.0
    @test plt.series[1].y[3] == 30.0
end

@testset "Plots: push!(plt, ...) mutates only the plot-owned series buffers" begin
    xs = [1.0, 2.0]
    ys = [10.0, 20.0]
    plt = plot(xs, ys)

    push!(plt, 3.0, 30.0)

    @test xs == [1.0, 2.0]
    @test ys == [10.0, 20.0]
    @test plt.series[1].x == [1.0, 2.0, 3.0]
    @test plt.series[1].y == [10.0, 20.0, 30.0]
end

@testset "Plots: push!(plt, x, y, z) appends a 3D point to series 1" begin
    plt = plot([1.0, 2.0], [1.0, 2.0], [1.0, 2.0])
    push!(plt, 3.0, 3.0, 3.0)
    @test length(plt.series[1].x) == 3
    @test length(plt.series[1].y) == 3
    @test length(plt.series[1].z) == 3
    @test plt.series[1].x[3] == 3.0
    @test plt.series[1].y[3] == 3.0
    @test plt.series[1].z[3] == 3.0
    @test plt.series[1].seriestype === :path3d
end

@testset "Plots: push!(plt, x, y, z) onto an empty plot3d series" begin
    plt = plot3d(1)
    @test length(plt.series[1].x) == 0
    push!(plt, 1.0, 2.0, 3.0)
    push!(plt, 4.0, 5.0, 6.0)
    @test length(plt.series[1].x) == 2
    @test plt.series[1].x[1] == 1.0
    @test plt.series[1].y[2] == 5.0
    @test plt.series[1].z[2] == 6.0
end

true
