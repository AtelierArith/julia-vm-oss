using Test
using Plots

@testset "Plots: plot(p::Plot) copies existing plot" begin
    p = plot([1.0, 2.0, 3.0]; aspect_ratio=:equal)
    q = plot(p)

    @test q.series !== p.series
    @test q.series[1].x == p.series[1].x
    @test q.series[1].y == p.series[1].y
    @test q.series[1].x !== p.series[1].x
    @test q.series[1].y !== p.series[1].y
    @test q.backend === p.backend
    @test q.aspect_ratio === :equal

    r = plot(q; aspect_ratio=:none)
    @test r.series !== q.series
    @test r.aspect_ratio == :none

    appended = plot!([4.0, 5.0])
    @test length(appended.series) == 2
    @test length(p.series) == 1
    @test length(q.series) == 1

    push!(r, 1, 9.0)
    @test length(r.series[1].y) == 4
    @test length(q.series[1].y) == 3
    @test length(p.series[1].y) == 3
end

true
