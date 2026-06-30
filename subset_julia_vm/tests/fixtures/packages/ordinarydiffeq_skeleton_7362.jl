# OrdinaryDiffEq README MVP package skeleton (Issue #7362).

using Test
using OrdinaryDiffEq

@testset "OrdinaryDiffEq package skeleton" begin
    @test isdefined(OrdinaryDiffEq, :solve)
    @test isdefined(SciMLBase, :solve)
    @test isdefined(OrdinaryDiffEq, :ODEProblem)
    @test isdefined(SciMLBase, :ODEProblem)
    @test isdefined(OrdinaryDiffEq, :ODESolution)
    @test isdefined(SciMLBase, :ODESolution)

    alg = Tsit5()
    @test alg isa Tsit5
    # Tsit5 is defined in SciMLBase (next to its `solve` dispatch) and re-exported
    # by OrdinaryDiffEq, so the algorithm registers with the base `solve` function
    # (Issue #7996 / PR #8050 review). The unqualified `Tsit5` re-export above is
    # the supported access path; `SciMLBase.Tsit5` is the definition site.
    @test alg isa SciMLBase.Tsit5
    @test alg.stage_limiter === nothing
    @test alg.step_limiter === nothing
    @test alg.thread === :serial
end

@testset "scalar ODEProblem skeleton" begin
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))

    @test prob isa SciMLBase.ODEProblem
    @test prob isa SciMLBase.AbstractODEProblem
    @test prob.u0 == 0.5
    @test prob.tspan == (0.0, 1.0)
    @test prob.p isa SciMLBase.NullParameters
    @test prob.isinplace == false

    prob_with_kw = ODEProblem(f, 0.5, (0.0, 1.0); reltol=1e-8, abstol=1e-8)
    @test prob_with_kw.u0 == 0.5
    @test prob_with_kw.tspan == (0.0, 1.0)
end

@testset "vector in-place ODEProblem skeleton" begin
    function lorenz!(du, u, p, t)
        du[1] = 10.0 * (u[2] - u[1])
        du[2] = u[1] * (28.0 - u[3]) - u[2]
        du[3] = u[1] * u[2] - (8 / 3) * u[3]
    end

    u0 = [1.0, 0.0, 0.0]
    prob = ODEProblem(lorenz!, u0, (0.0, 100.0))

    @test prob.u0 == u0
    @test prob.tspan == (0.0, 100.0)
    @test prob.p isa SciMLBase.NullParameters
    @test prob.isinplace == true
end

true
