using Test
using Plots

# Issue #6355: push!(plt, i, y) extends series i in place, auto-extending x by 1.
@testset "Plots: push!(p, i, y) extends series i" begin
    p = plot(1)
    @test length(p.series[1].y) == 1
    @test p.series[1].y[1] == 1.0

    push!(p, 1, 2.0)
    push!(p, 1, 3.0)

    @test length(p.series[1].y) == 3
    @test p.series[1].y[2] == 2.0
    @test p.series[1].y[3] == 3.0
    # x is auto-extended by +1 from the last x on each push.
    @test p.series[1].x[2] == 2.0
    @test p.series[1].x[3] == 3.0
end

true
