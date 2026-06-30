using Test
using Plots

@testset "Plots: plot(sin)" begin
    p = plot(sin)
    @test length(p.series) == 1
    s = p.series[1]
    @test length(s.x) == 100
    @test length(s.y) == 100

    # Match upstream Plots.jl default xlims = (-5, 5) (Issue #4364).
    @test isapprox(s.x[1], -5.0)
    @test isapprox(s.x[end], 5.0)
    @test isapprox(s.y[1], sin(-5.0))
    @test isapprox(s.y[end], sin(5.0))
end

true
