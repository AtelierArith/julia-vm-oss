using Test
using Plots

# Issue #7322: `scatter(M::AbstractMatrix)` (and `scatter!`) plots one series per
# column, matching upstream Plots.jl. Before the fix the bundled Plots only
# defined `scatter(y::Vector)` / `scatter(x, y)`, so passing a matrix raised
# `MethodError: no method matching scatter(::Matrix{Float64})` — which is exactly
# what blocked the #7275 Interact `@manipulate` sample (`scatter(rand(10, 2))`).
@testset "Plots: scatter(::Matrix) one series per column (#7322)" begin
    m = [1.0 2.0 3.0; 4.0 5.0 6.0]   # 2x3 -> 3 series of 2 points each

    p = scatter(m)
    @test p isa Plot
    @test length(p.series) == 3

    # x is the row index 1:size(m, 1) shared by every column series.
    @test p.series[1].x == [1, 2]
    @test p.series[2].x == [1, 2]

    # Each column becomes its own :scatter series, in column order.
    @test p.series[1].seriestype === :scatter
    @test p.series[2].seriestype === :scatter
    @test p.series[3].seriestype === :scatter
    @test p.series[1].y == [1.0, 4.0]
    @test p.series[2].y == [2.0, 5.0]
    @test p.series[3].y == [3.0, 6.0]

    # A single-column matrix is one series.
    p1 = scatter(reshape([7.0, 8.0, 9.0], 3, 1))
    @test length(p1.series) == 1
    @test p1.series[1].y == [7.0, 8.0, 9.0]

    # `scatter!(::Matrix)` appends one series per column to the current plot.
    base = scatter([0.0, 0.0])
    @test length(base.series) == 1
    appended = scatter!([1.0 2.0; 3.0 4.0])
    @test length(appended.series) == 3
    @test appended.series[2].y == [1.0, 3.0]
    @test appended.series[3].y == [2.0, 4.0]

    # The #7275 carrier form dispatches (rand is nondeterministic: assert shape).
    pr = scatter(rand(10, 2))
    @test pr isa Plot
    @test length(pr.series) == 2
    @test length(pr.series[1].y) == 10
    @test length(pr.series[2].y) == 10
end

true
