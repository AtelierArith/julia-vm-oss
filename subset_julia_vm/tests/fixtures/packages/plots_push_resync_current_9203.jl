using Test
using Plots

# Issue #9203: `push!(plt, …)` fast-path invariant.
#
# `push!(plt, …)` extends the plot's series in place and must keep `current()`
# reflecting the mutated figure (Issue #8214). The fast path in
# `_plots_resync_current!` skips re-publishing all 7 `_CURRENT_*` holders on every
# push once `plt` is already the current figure — which is only correct because
# sjulia stores the series into `_CURRENT_SERIES[1]` by reference (so in-place
# extends are visible through `current()` without a fresh republish). These tests
# lock that invariant: if the aliasing ever breaks, the fast path would silently
# capture empty/stale animation frames (the original #8214 failure), so this must
# fail loudly instead.
@testset "Plots #9203: current() reflects push!(plt) without per-push republish" begin
    plt = plot3d(1, legend=false)
    @test length(Plots.current().series[1].x) == 0
    for i in 1:50
        push!(plt, Float64(i), Float64(2i), Float64(3i))
    end
    # The pushed plot stays current and current() sees every in-place extend.
    @test length(plt.series[1].x) == 50
    @test length(Plots.current().series[1].x) == 50
    @test length(Plots.current().series[1].z) == 50
    @test Plots.current().series[1].x[50] == 50.0
    @test Plots.current().series[1].z[50] == 150.0
    # current() must alias the same live series buffer that push! mutates.
    @test Plots.current().series === plt.series
end

@testset "Plots #9203: pushing a non-current plot re-publishes it as current" begin
    a = plot3d(1, legend=false)
    push!(a, 1.0, 1.0, 1.0)
    b = plot3d(1, legend=false)          # b becomes current
    @test Plots.current().series === b.series
    push!(a, 2.0, 2.0, 2.0)              # pushing a must make a current again
    @test Plots.current().series === a.series
    @test length(Plots.current().series[1].x) == 2
end

true
