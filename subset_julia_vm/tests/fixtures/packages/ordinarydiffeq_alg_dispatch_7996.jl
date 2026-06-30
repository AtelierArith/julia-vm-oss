# OrdinaryDiffEq alg dispatch: solve dispatches on alg type (Issue #7996).
# Tsit5 is supported; passing an unknown algorithm raises an explicit error.

using Test
using OrdinaryDiffEq

struct UnsupportedAlg end

@testset "OrdinaryDiffEq alg dispatch (Issue #7996)" begin
    f(u, p, t) = -u
    prob = ODEProblem(f, 1.0, (0.0, 1.0))

    # Tsit5 must dispatch correctly and return a valid solution.
    sol = solve(prob, Tsit5(); dt=0.1)
    @test sol isa SciMLBase.ODESolution
    @test successful_retcode(sol)
    @test sol.alg isa Tsit5
    @test sol.stats[:algorithm] === :Tsit5

    # An unsupported algorithm must raise an explicit error, not silently use Tsit5.
    @test_throws ErrorException solve(prob, UnsupportedAlg())

    # The Tsit5 method is registered ON SciMLBase.solve, so qualifying the public
    # API the same way upstream solver packages do must dispatch identically —
    # not fall through to the generic unsupported-alg error (Issue #8050 review).
    qsol = SciMLBase.solve(prob, Tsit5(); dt=0.1)
    @test qsol isa SciMLBase.ODESolution
    @test successful_retcode(qsol)
    @test qsol.alg isa Tsit5
    @test qsol.stats[:algorithm] === :Tsit5

    # And the qualified entry point still rejects unknown algorithms.
    @test_throws ErrorException SciMLBase.solve(prob, UnsupportedAlg())
end

true
