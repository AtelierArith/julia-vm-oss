# OrdinaryDiffEq README MVP completion fixture (Issue #7366).

using Test
using OrdinaryDiffEq
using Plots

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

@testset "linear README MVP plot with analytical overlay" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 1 / 2, (0.0, 1.0))
    sol = solve(prob, Tsit5(), dt=0.1, reltol=1e-8, abstol=1e-8)

    plt = plot(sol, linewidth=5, title="Solution to the linear ODE with a thick line",
               xaxis="Time (t)", yaxis="u(t)", label="My Thick Line!")
    plt = plot!(sol.t, t -> 0.5 * exp(1.01 * t), lw=3, ls=:dash,
                label="True Solution!")

    @test plt isa Plot
    @test length(plt.series) == 2
    @test plt.series[1].x == sol.t
    @test plt.series[1].y == sol.u
    @test abs(plt.series[2].y[end] - 1.3728005075084582) < 1e-12
end

@testset "Lorenz README MVP 3D path plot" begin
    prob = ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.2))
    sol = solve(prob, Tsit5(), dt=0.01, saveat=0.01)
    plt = plot(sol, idxs=(1, 2, 3))

    @test plt isa Plot
    @test length(plt.series) == 1
    @test plt.series[1].seriestype === :path3d
    @test plt.series[1].x[1] == 1.0
    @test plt.series[1].y[1] == 0.0
    @test plt.series[1].z[1] == 0.0
    @test length(plt.series[1].x) == length(sol.t)
end

true
