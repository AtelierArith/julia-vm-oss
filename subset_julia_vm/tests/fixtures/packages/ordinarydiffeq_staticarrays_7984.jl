# OrdinaryDiffEq StaticArrays README variant (Issue #7984): an out-of-place RHS
# returning an `SVector` with an `@SVector` initial state, solved through the same
# `solve(prob, Tsit5(); ...)` surface as the dynamic-`Vector` version. The static
# element type must be preserved end-to-end (no silent widening to `Vector`) and the
# trajectory must match the dynamic version within tolerance.
#
# NOTE: states are compared ELEMENT-WISE (`u[i]`), not via `su .- du`, because
# broadcasting a mixed `SVector .- Vector` pair mis-lowers in sjulia (Issue #8161).

using Test
using OrdinaryDiffEq
using StaticArrays

# Lorenz system, out-of-place, returning a static 3-vector (README StaticArrays form).
function lorenz_static(u, p, t)
    return @SVector [10.0 * (u[2] - u[1]),
                     u[1] * (28.0 - u[3]) - u[2],
                     u[1] * u[2] - (8 / 3) * u[3]]
end

# Same system, in-place, dynamic `Vector` state (the MVP form) for parity comparison.
function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

tspan = (0.0, 1.0)

prob_s = ODEProblem(lorenz_static, (@SVector [1.0, 0.0, 0.0]), tspan)
sol_s = solve(prob_s, Tsit5(); dt=0.05, saveat=0.05)

prob_d = ODEProblem(lorenz!, [1.0, 0.0, 0.0], tspan)
sol_d = solve(prob_d, Tsit5(); dt=0.05, saveat=0.05)

# Largest element-wise difference between the static and dynamic trajectories.
function _max_traj_diff(sols, sold)
    m = 0.0
    for k in 1:length(sols.u)
        us = sols.u[k]
        ud = sold.u[k]
        for i in 1:length(us)
            d = abs(us[i] - ud[i])
            if d > m
                m = d
            end
        end
    end
    return m
end

@testset "StaticArrays variant: type preservation" begin
    # the saved state stays a static 3-vector — NOT silently widened to Vector
    @test sol_s.u[end] isa SVector
    @test length(sol_s.u[end]) == 3
    @test sol_s.u[1] isa SVector
    # the dynamic reference is a plain Vector
    @test sol_d.u[end] isa Vector
end

@testset "StaticArrays variant: trajectory parity with dynamic Vector" begin
    @test length(sol_s.u) == length(sol_d.u)
    @test sol_s.t == sol_d.t
    # static and dynamic integrations agree to tight tolerance (same stepper core)
    @test _max_traj_diff(sol_s, sol_d) < 1e-9
end

# Regression gate (Issue #8158 fixture-weakness lesson): end with a boolean computed
# from the actual results so a regression (widening or a static/dynamic divergence)
# flips the script's final value to `false`. `@test` failures alone do not throw.
(sol_s.u[end] isa SVector) && (sol_s.t == sol_d.t) && (_max_traj_diff(sol_s, sol_d) < 1e-9)
