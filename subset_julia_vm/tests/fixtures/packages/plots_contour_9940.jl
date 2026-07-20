using Test
using Plots

function _plots_fixture_seriestype(p, i)
    if hasproperty(p, :series)
        return p.series[i].seriestype
    end
    return p[1][i].plotattributes[:seriestype]
end

function _plots_fixture_levels(p, i)
    if hasproperty(p, :series)
        return p.series[i].levels
    end
    return p[1][i].plotattributes[:levels]
end

@testset "Plots: contour and contour! (Issue #9940)" begin
    z = [1.0 2.0; 3.0 4.0]
    p = contour(z)
    @test p isa Plots.Plot
    @test _plots_fixture_seriestype(p, 1) === :contour
    if hasproperty(p, :series)
        @test length(p.series) == 1
        s = p.series[1]
        @test s.x == [1, 2]
        @test s.y == [1, 2]
        @test s.z[1, 1] == 1.0
        @test s.z[2, 2] == 4.0
        @test s.levels === nothing
    end

    xs = [10.0, 20.0]
    ys = [1.0, 2.0, 3.0]
    p2 = contour(xs, ys, (x, y) -> y * 100 + x; levels=3, aspect_ratio=:equal, label="H")
    @test _plots_fixture_seriestype(p2, 1) === :contour
    @test _plots_fixture_levels(p2, 1) == 3
    if hasproperty(p2, :series)
        s2 = p2.series[1]
        @test s2.x == xs
        @test s2.y == ys
        @test s2.z[1, 1] == 110.0
        @test s2.z[2, 1] == 210.0
        @test s2.z[3, 2] == 320.0
        @test s2.label == "H"
        @test p2.aspect_ratio === :equal
    end

    z2 = [110.0 120.0; 210.0 220.0; 310.0 320.0]
    base = heatmap(xs, ys, z2)
    p3 = contour!(xs, ys, z2; levels=-1.0:1.0:3.0, title="phase")
    @test base isa Plots.Plot
    @test p3 isa Plots.Plot
    @test _plots_fixture_seriestype(p3, 2) === :contour
    if hasproperty(p3, :series)
        @test length(p3.series) == 2
        @test p3.series[1].seriestype === :heatmap
        @test collect(p3.series[2].levels) == [-1.0, 0.0, 1.0, 2.0, 3.0]
        @test p3.title == "phase"
    end

    @test_throws ArgumentError contour(xs, ys, z2; levels=1.5)
    @test_throws ArgumentError contour(xs, ys, z2; levels=-3)
end

true
