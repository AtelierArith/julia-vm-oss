using Test
using Plots

@testset "Plots: scatter! appends :scatter series to current plot" begin
    # User-reported workflow: plot, plot!, then scatter! must coexist.
    p = plot(sin)
    plot!(cos)
    # Workaround: use literal vectors instead of collect(1:n) here. Comparing a
    # struct-field-loaded Vector{Int64} that originated from collect(...) against
    # the source variable fails with MethodError (Issue #4446); literal vectors
    # are unaffected.
    xs = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    ys = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
    p3 = scatter!(xs, ys)
    @test length(p3.series) == 3
    @test p3.series[1].seriestype === :line
    @test p3.series[2].seriestype === :line
    @test p3.series[3].seriestype === :scatter
    @test p3.series[3].x == xs
    @test p3.series[3].y == ys

    # scatter!(f) uses the same (-5, 5) default sampling as plot!(f).
    p4 = scatter!(tan)
    s4 = p4.series[end]
    @test s4.seriestype === :scatter
    @test length(s4.x) == 100
    @test isapprox(s4.x[1], -5.0)
    @test isapprox(s4.x[end], 5.0)
    @test isapprox(s4.y[1], tan(-5.0))

    # scatter!(y::Vector) uses 1:length(y) for x.
    ys2 = [10.0, 20.0, 30.0]
    p5 = scatter!(ys2)
    s5 = p5.series[end]
    @test s5.seriestype === :scatter
    @test s5.x == [1, 2, 3]
    @test s5.y == ys2

    # scatter() now seeds _CURRENT_SERIES so a follow-up scatter!/plot! appends
    # rather than silently creating a detached plot.
    ps = scatter([1, 2, 3], [4.0, 5.0, 6.0])
    @test length(ps.series) == 1
    psb = scatter!([4, 5], [7.0, 8.0])
    @test length(psb.series) == 2
    @test psb.series[1].seriestype === :scatter
    @test psb.series[2].seriestype === :scatter
    pl = plot!([6, 7], [9.0, 10.0])
    @test length(pl.series) == 3
    @test pl.series[3].seriestype === :line
end

true
