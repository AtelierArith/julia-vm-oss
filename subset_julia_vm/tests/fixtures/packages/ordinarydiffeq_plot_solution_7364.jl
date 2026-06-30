# OrdinaryDiffEq ODESolution plotting through bundled Plots (Issue #7364).

using Test
using OrdinaryDiffEq
using Plots

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

@testset "plot scalar ODESolution and overlay" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))
    sol = solve(prob, Tsit5(); dt=0.1)

    p = plot(sol, linewidth=5, title="linear", xaxis="Time", yaxis="u(t)", label="Tsit5")
    @test p isa Plot
    @test p.title == "linear"
    @test length(p.series) == 1
    @test p.series[1].seriestype === :line
    @test p.series[1].x == sol.t
    @test p.series[1].y == sol.u

    p2 = plot!(sol.t, t -> 0.5 * exp(1.01 * t), lw=3, ls=:dash, label="True")
    @test length(p2.series) == 2
    @test p2.series[2].x == sol.t
    @test abs(p2.series[2].y[end] - 1.3728005075084582) < 1e-12
end

@testset "plot vector ODESolution components and phases" begin
    u0 = [1.0, 0.0, 0.0]
    prob = ODEProblem(lorenz!, u0, (0.0, 0.2))
    sol = solve(prob, Tsit5(); dt=0.01)

    p = plot(sol)
    @test length(p.series) == 3
    @test p.series[1].x == sol.t
    @test p.series[1].y[1] == 1.0
    @test p.series[2].y[1] == 0.0
    @test p.series[3].y[1] == 0.0

    phase2 = plot(sol, idxs=(1, 2))
    @test length(phase2.series) == 1
    @test phase2.series[1].seriestype === :line
    @test phase2.series[1].x[1] == 1.0
    @test phase2.series[1].y[1] == 0.0

    phase3 = plot(sol, idxs=(1, 2, 3))
    @test length(phase3.series) == 1
    @test phase3.series[1].seriestype === :path3d
    @test phase3.series[1].x[1] == 1.0
    @test phase3.series[1].y[1] == 0.0
    @test phase3.series[1].z[1] == 0.0
    @test length(phase3.series[1].x) == length(sol.t)
end

true
