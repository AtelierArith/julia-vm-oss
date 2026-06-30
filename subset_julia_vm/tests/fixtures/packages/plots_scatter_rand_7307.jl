using Test
using Plots

# Issue #7307: a native-array carrier produced by `rand(n)` / `collect(...)`
# must dispatch into `scatter(y::Vector)` / `plot(y::Vector)` exactly like a
# literal `Vector` does. Before the fix `rand(5)` inferred to the
# unparameterized `Array` (rank unknown) and the static dispatcher raised
# `MethodError: no method matching scatter(::Array)` / `scatter(::Float64)`,
# while `collect(rand(5))` fell to a runtime miss.
#
# `rand` is nondeterministic, so we assert structure (typeof / length / series
# count / seriestype), never the sampled values.
@testset "Plots: scatter/plot over rand & collect carriers (#7307)" begin
    # rand(n) is a Vector{Float64} carrier.
    y = rand(5)
    @test y isa Vector{Float64}
    @test typeof(y) === Vector{Float64}

    # scatter(rand(n)) dispatches to scatter(y::Vector).
    ps = scatter(rand(5))
    @test ps isa Plot
    @test length(ps.series) == 1
    ss = ps.series[1]
    @test ss.seriestype === :scatter
    @test length(ss.x) == 5
    @test length(ss.y) == 5

    # plot(rand(n)) dispatches to plot(y::Vector).
    pp = plot(rand(7))
    @test pp isa Plot
    @test length(pp.series) == 1
    @test length(pp.series[1].y) == 7
    @test pp.series[1].seriestype === :line

    # collect(rand(n)) is a plain Array carrier and must dispatch the same way.
    pc = scatter(collect(rand(4)))
    @test pc isa Plot
    @test pc.series[1].seriestype === :scatter
    @test length(pc.series[1].y) == 4

    # randn(n) shares the same carrier shape.
    pr = scatter(randn(6))
    @test pr isa Plot
    @test length(pr.series[1].y) == 6
end

true
