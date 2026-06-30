# OrdinaryDiffEq integrator interface — remake (Issue #7981). `remake(prob; ...)`
# returns a new ODEProblem with the overridden fields and a re-derived `isinplace`,
# leaving unspecified fields at their previous value. Solving the remade problem
# follows the new fields.

using Test
using OrdinaryDiffEq

# in-place exponential decay: u' = -u  ->  u(t) = u0 * exp(-t)
function decay!(du, u, p, t)
    du[1] = -u[1]
end

base = ODEProblem(decay!, [1.0], (0.0, 1.0))

@testset "remake overrides u0, keeps other fields" begin
    prob2 = remake(base; u0=[2.0])
    @test prob2.u0 == [2.0]
    @test prob2.tspan == (0.0, 1.0)        # unchanged
    @test prob2.f === base.f               # unchanged
    @test prob2.isinplace == true          # re-derived
    sol = solve(prob2, Tsit5(); dt=0.1, saveat=0.1)
    @test abs(sol.u[end][1] - 2.0 * exp(-1.0)) < 1e-3
end

@testset "remake overrides tspan" begin
    prob3 = remake(base; tspan=(0.0, 2.0))
    @test prob3.tspan == (0.0, 2.0)
    @test prob3.u0 == [1.0]                # unchanged
    sol = solve(prob3, Tsit5(); dt=0.1, saveat=0.1)
    @test sol.t[end] == 2.0
    @test abs(sol.u[end][1] - exp(-2.0)) < 1e-3
end

@testset "remake re-derives isinplace for an out-of-place f" begin
    oop(u, p, t) = -u                       # scalar out-of-place
    prob4 = remake(base; f=oop, u0=1.0)
    @test prob4.isinplace == false
    sol = solve(prob4, Tsit5(); dt=0.1, saveat=0.1)
    @test abs(sol.u[end] - exp(-1.0)) < 1e-3
end

# Regression gate: end with a boolean computed from the actual remade solves.
function _remake_gate()
    p2 = remake(base; u0=[2.0])
    s2 = solve(p2, Tsit5(); dt=0.1, saveat=0.1)
    p3 = remake(base; tspan=(0.0, 2.0))
    s3 = solve(p3, Tsit5(); dt=0.1, saveat=0.1)
    return p2.u0 == [2.0] && p2.tspan == (0.0, 1.0) && p2.isinplace == true &&
           abs(s2.u[end][1] - 2.0 * exp(-1.0)) < 1e-3 &&
           p3.tspan == (0.0, 2.0) && s3.t[end] == 2.0 &&
           abs(s3.u[end][1] - exp(-2.0)) < 1e-3
end

_remake_gate()
