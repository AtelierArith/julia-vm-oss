# OrdinaryDiffEq README MVP in-place vector solve path (Issues #7363/#7367).

using Test
using OrdinaryDiffEq

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

@testset "OrdinaryDiffEq Lorenz Tsit5 solve" begin
    u0 = [1.0, 0.0, 0.0]
    prob = ODEProblem(lorenz!, u0, (0.0, 0.2))
    sol = solve(prob, Tsit5(); dt=0.01, saveat=0.01)

    @test sol isa SciMLBase.ODESolution
    @test successful_retcode(sol)
    @test sol.prob === prob
    @test sol.alg isa Tsit5
    @test sol.stats[:algorithm] === :Tsit5
    @test sol.stats[:steps] == 20
    @test sol.stats[:attempts] == 20
    @test sol.stats[:rejected_steps] == 0
    @test sol.stats[:rhs_evals] == 121

    @test length(sol.t) == 21
    @test length(sol.u) == 21
    @test sol.t[1] == 0.0
    @test abs(sol.t[end] - 0.2) < 1e-12
    @test sol.u[1] == [1.0, 0.0, 0.0]
    @test u0 == [1.0, 0.0, 0.0]

    last = sol.u[end]
    @test length(last) == 3
    @test abs(last[1] - 3.913185971000745) < 1e-12
    @test abs(last[2] - 8.433170579932218) < 1e-12
    @test abs(last[3] - 1.2683601703724348) < 1e-12
end

true
