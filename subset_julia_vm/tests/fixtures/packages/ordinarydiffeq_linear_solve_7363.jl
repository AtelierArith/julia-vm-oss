# OrdinaryDiffEq README MVP scalar solve path (Issues #7363/#7367).

using Test
using OrdinaryDiffEq

@testset "OrdinaryDiffEq scalar Tsit5 solve" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))
    sol = solve(prob, Tsit5(); dt=0.1, reltol=1e-8, abstol=1e-8)

    @test sol isa SciMLBase.ODESolution
    @test sol.prob === prob
    @test sol.alg isa Tsit5
    @test successful_retcode(sol)
    @test sol.stats[:algorithm] === :Tsit5
    @test sol.stats[:steps] == 10
    @test sol.stats[:attempts] == 10
    @test sol.stats[:rejected_steps] == 0
    @test sol.stats[:rhs_evals] == 61

    @test length(sol.t) == 11
    @test length(sol.u) == 11
    @test sol.t[1] == 0.0
    @test sol.t[end] == 1.0
    @test sol.u[1] == 0.5
    @test abs(sol.u[end] - 1.372800507811782) < 1e-12
end

true
