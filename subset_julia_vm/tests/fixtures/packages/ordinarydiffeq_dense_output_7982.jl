# OrdinaryDiffEq dense output / continuous interpolation (Issue #7982): the
# callable ODESolution sol(t) / sol(t; idxs=...) / sol(ts). MVP uses linear
# interpolation between saved grid points (not the Tsit5 dense interpolant).

using Test
using OrdinaryDiffEq

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

@testset "dense linear interpolation (scalar)" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))
    sol = solve(prob, Tsit5(); dt=0.1, saveat=0.1)

    # exact at saved grid points
    @test sol(0.0) == sol.u[1]
    @test sol(1.0) == sol.u[end]

    # off-grid: close to the analytic solution 0.5*exp(1.01*t)
    @test abs(sol(0.55) - 0.5 * exp(1.01 * 0.55)) < 0.02
    @test abs(sol(0.25) - 0.5 * exp(1.01 * 0.25)) < 0.02

    # midpoint of a save interval is the chord midpoint
    mid = 0.5 * (sol.u[1] + sol.u[2])
    @test abs(sol(0.05) - mid) < 1e-12

    # outside tspan clamps to the endpoints
    @test sol(-1.0) == sol.u[1]
    @test sol(2.0) == sol.u[end]
end

@testset "dense interpolation (vector + idxs)" begin
    prob = ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.2))
    sol = solve(prob, Tsit5(); dt=0.01, saveat=0.01)

    s = sol(0.105)
    @test length(s) == 3
    @test sol(0.105; idxs=2) == s[2]
    @test sol(0.105; idxs=(1, 3)) == [s[1], s[3]]

    # endpoints clamp to the saved states exactly
    @test sol(0.0) == sol.u[1]
    @test sol(0.2) == sol.u[end]
end

@testset "vectorized sampling sol(ts)" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))
    sol = solve(prob, Tsit5(); dt=0.1, saveat=0.1)

    pts = sol([0.0, 0.5, 1.0])
    @test length(pts) == 3
    @test pts[1] == sol.u[1]
    @test pts[3] == sol.u[end]
    @test abs(pts[2] - 0.5 * exp(1.01 * 0.5)) < 0.02
end

# Regression gate: end with a boolean computed from the actual interpolation so a
# regression flips the script's final value to `false` (the fixture harness only
# checks the final value, and sjulia `@test` failures print but do not throw).
function _dense_output_gate()
    f(u, p, t) = 1.01 * u
    sol = solve(ODEProblem(f, 0.5, (0.0, 1.0)), Tsit5(); dt=0.1, saveat=0.1)
    on_grid = sol(0.0) == sol.u[1] && sol(1.0) == sol.u[end]
    off_grid = abs(sol(0.55) - 0.5 * exp(1.01 * 0.55)) < 0.02
    clamps = sol(-1.0) == sol.u[1] && sol(2.0) == sol.u[end]
    solv = solve(ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.2)), Tsit5();
                 dt=0.01, saveat=0.01)
    idxs_ok = solv(0.105; idxs=2) == solv(0.105)[2]
    return on_grid && off_grid && clamps && idxs_ok
end

_dense_output_gate()
