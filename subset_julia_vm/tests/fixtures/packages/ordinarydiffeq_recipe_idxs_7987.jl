# OrdinaryDiffEq Plots recipe attributes (Issue #7987): `idxs` (component / phase
# selection), `vars` (upstream's deprecated alias for `idxs`), and
# `denseplot`/`plotdensity` (sample the callable solution sol(t), #7982) all flow
# through the `apply_recipe` recipe path, plus `plot!(sol)` overlay.

using Test
using OrdinaryDiffEq
using Plots

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

sol = solve(ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.2)), Tsit5(); dt=0.01)

@testset "idxs phase plots through the recipe" begin
    p2 = plot(sol, idxs=(1, 2))
    @test length(p2.series) == 1
    @test p2.series[1].seriestype === :line
    @test p2.series[1].x[1] == 1.0
    @test p2.series[1].y[1] == 0.0

    p3 = plot(sol, idxs=(1, 2, 3))
    @test length(p3.series) == 1
    @test p3.series[1].seriestype === :path3d
    @test p3.series[1].z !== nothing
    @test p3.series[1].z[1] == 0.0
    @test length(p3.series[1].x) == length(sol.t)

    p1 = plot(sol, idxs=2)              # single component vs t
    @test length(p1.series) == 1
    @test p1.series[1].x == sol.t
end

@testset "vars is the deprecated alias for idxs" begin
    pv = plot(sol, vars=(1, 2))
    pi = plot(sol, idxs=(1, 2))
    @test pv.series[1].x == pi.series[1].x
    @test pv.series[1].y == pi.series[1].y
end

@testset "denseplot samples sol(t) on a fine grid" begin
    pd = plot(sol, denseplot=true, plotdensity=50)
    @test length(pd.series) == 3
    # a finer grid than the saved points
    @test length(pd.series[1].x) == 50
    @test length(sol.t) < 50
    # first/last dense points coincide with the solution endpoints
    @test abs(pd.series[1].x[1] - sol.t[1]) < 1e-12
    @test abs(pd.series[1].x[end] - sol.t[end]) < 1e-12
end

@testset "plot!(sol) overlays recipe series" begin
    plot(sol)                           # 3 component series
    po = plot!(sol, idxs=(1, 2))        # + 1 phase series
    @test length(po.series) == 4
end

# Regression gate.
function _recipe_idxs_gate()
    p3 = plot(sol, idxs=(1, 2, 3))
    phase_ok = length(p3.series) == 1 && p3.series[1].seriestype === :path3d &&
               p3.series[1].z !== nothing
    pv = plot(sol, vars=(1, 2))
    pi = plot(sol, idxs=(1, 2))
    vars_ok = pv.series[1].x == pi.series[1].x
    pd = plot(sol, denseplot=true, plotdensity=50)
    dense_ok = length(pd.series[1].x) == 50
    return phase_ok && vars_ok && dense_ok
end

_recipe_idxs_gate()
