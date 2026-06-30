# OrdinaryDiffEq Tsit5 adaptive stepping regression (Issue #7367).

using Test
using OrdinaryDiffEq

@testset "Tsit5 tolerances affect adaptive stepping" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))

    loose = solve(prob, Tsit5(); dt=0.5, saveat=0.5, reltol=1e-2, abstol=1e-6)
    tight = solve(prob, Tsit5(); dt=0.5, saveat=0.5, reltol=1e-10, abstol=1e-12)

    @test loose.stats[:algorithm] === :Tsit5
    @test tight.stats[:algorithm] === :Tsit5
    @test loose.t == [0.0, 0.5, 1.0]
    @test tight.t == [0.0, 0.5, 1.0]
    @test loose.stats[:steps] == 2
    @test loose.stats[:rhs_evals] == 13
    @test tight.stats[:steps] == 26
    @test tight.stats[:attempts] == 28
    @test tight.stats[:rejected_steps] == 2
    @test tight.stats[:rhs_evals] == 169
    @test tight.stats[:steps] > loose.stats[:steps]
    @test tight.stats[:rhs_evals] > loose.stats[:rhs_evals]
    @test abs(tight.u[end] - 1.3728005075084582) < 1e-10
    @test abs(loose.u[end] - tight.u[end]) > 1e-7
end

true
