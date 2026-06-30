using Test
using Plots

@testset "Plots: heatmap rectangular array (Issue #6360)" begin
    z = [1.0 2.0; 3.0 4.0]
    p = heatmap(z)
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :heatmap
    @test s.x == [1, 2]
    @test s.y == [1, 2]
    @test s.z[1, 1] == 1.0
    @test s.z[2, 2] == 4.0

    xs = [10.0, 20.0]
    ys = [1.0, 2.0, 3.0]
    z2 = [110.0 120.0; 210.0 220.0; 310.0 320.0]
    p2 = heatmap(xs, ys, z2, aspect_ratio=:equal)
    @test length(p2.series) == 1
    @test p2.series[1].seriestype === :heatmap
    @test p2.series[1].x == xs
    @test p2.series[1].y == ys
    @test p2.series[1].z[3, 2] == 320.0
    @test p2.aspect_ratio === :equal

    p3 = plot([1.0, 2.0], [3.0, 4.0], aspect_ratio=:equal)
    p4 = heatmap!(xs, ys, z2)
    @test length(p3.series) == 2
    @test length(p4.series) == 2
    @test p4.series[2].seriestype === :heatmap
    @test p4.aspect_ratio === :equal
end

true
