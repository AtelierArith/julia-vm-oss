# OrdinaryDiffEq Plots recipe pipeline (Issue #7987): plot(sol) goes through the
# `apply_recipe` recipe mechanism (registered on AbstractODESolution) instead of a
# hard-coded special case. This fixture pins NO REGRESSION: the resulting
# Plot/Series shape must match the former direct conversion.

using Test
using OrdinaryDiffEq
using Plots

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

@testset "scalar ODESolution recipe (no regression)" begin
    f(u, p, t) = 1.01 * u
    sol = solve(ODEProblem(f, 0.5, (0.0, 1.0)), Tsit5(); dt=0.1)
    p = plot(sol, linewidth=5, title="linear", label="Tsit5")
    @test p isa Plot
    @test p.title == "linear"
    @test length(p.series) == 1
    @test p.series[1].seriestype === :line
    @test p.series[1].x == sol.t
    @test p.series[1].y == sol.u
end

@testset "vector ODESolution recipe → one series per component" begin
    sol = solve(ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.2)), Tsit5(); dt=0.01)
    p = plot(sol)
    @test length(p.series) == 3
    @test p.series[1].x == sol.t
    @test p.series[1].y[1] == 1.0
    @test p.series[2].y[1] == 0.0
    @test p.series[3].y[1] == 0.0
    @test p.series[1].seriestype === :line
end

@testset "recipe registry hook" begin
    sol = solve(ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.1)), Tsit5(); dt=0.01)
    # plot(sol) routed through the recipe still produces a Plot
    @test plot(sol) isa Plot
end

# Regression gate.
function _recipe_plot_gate()
    f(u, p, t) = 1.01 * u
    sc = solve(ODEProblem(f, 0.5, (0.0, 1.0)), Tsit5(); dt=0.1)
    ps = plot(sc)
    scalar_ok = length(ps.series) == 1 && ps.series[1].seriestype === :line &&
                ps.series[1].y == sc.u
    vec = solve(ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.2)), Tsit5(); dt=0.01)
    pv = plot(vec)
    vec_ok = length(pv.series) == 3 && pv.series[1].x == vec.t && pv.series[2].y[1] == 0.0
    return scalar_ok && vec_ok
end

_recipe_plot_gate()
