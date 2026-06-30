# OrdinaryDiffEq integrator interface subset (Issue #7981): init / step! /
# solve! / reinit! / remake / successful_retcode built on the adaptive Tsit5
# stepper. The integrator path must reproduce solve(prob, alg; ...).

using Test
using OrdinaryDiffEq

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

@testset "init/step!/solve! reproduce solve (scalar)" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))
    sol = solve(prob, Tsit5(); dt=0.1, saveat=0.1)

    integ = init(prob, Tsit5(); dt=0.1, saveat=0.1)
    @test integ.t == 0.0
    @test integ.u == 0.5

    @test step!(integ) == true
    @test integ.t == 0.1

    sol2 = solve!(integ)
    @test integ.t == 1.0
    @test sol2.t == sol.t
    @test sol2.u == sol.u
    @test successful_retcode(sol2)
    @test step!(integ) == false
end

@testset "init/solve! reproduce solve (vector in-place)" begin
    prob = ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.2))
    sol = solve(prob, Tsit5(); dt=0.01, saveat=0.01)
    sol2 = solve!(init(prob, Tsit5(); dt=0.01, saveat=0.01))
    @test sol2.t == sol.t
    @test length(sol2.u) == length(sol.u)
    @test sol2.u[end] == sol.u[end]
    @test successful_retcode(sol2)
end

@testset "remake overrides problem fields" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))
    prob2 = remake(prob; u0=1.0, tspan=(0.0, 2.0))
    @test prob2.u0 == 1.0
    @test prob2.tspan == (0.0, 2.0)
    # f is carried over from the original problem. Compare behaviour rather than
    # identity: sjulia does not preserve function `===` identity (Issue #7993).
    @test prob2.f(2.0, nothing, 0.0) == prob.f(2.0, nothing, 0.0)
    @test prob2.isinplace == false

    sol = solve(prob2, Tsit5(); dt=0.1)
    @test sol.u[1] == 1.0
    @test sol.t[end] == 2.0
    @test successful_retcode(sol)
end

@testset "reinit! resets integrator" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))
    integ = init(prob, Tsit5(); dt=0.1, saveat=0.1)
    sol_a = solve!(integ)

    reinit!(integ)
    @test integ.t == 0.0
    @test integ.u == 0.5

    sol_b = solve!(integ)
    @test sol_b.t == sol_a.t
    @test sol_b.u == sol_a.u
end

@testset "successful_retcode on symbol" begin
    @test successful_retcode(:Success)
    @test successful_retcode(:Terminated)
    @test !successful_retcode(:Failure)
end

true
