# Optim.jl MVP — derivative-free multivariate NelderMead (Issue #7481).

using Test
using Optim

@testset "Optim NelderMead MVP" begin
    f = x -> sum(abs2, x)

    r = optimize(f, [3.0, -1.0], NelderMead())

    # Converges to the upstream-compatible minimizer/minimum within tolerance.
    mz = Optim.minimizer(r)
    @test abs(mz[1]) < 1e-3
    @test abs(mz[2]) < 1e-3
    @test Optim.minimum(r) < 1e-6
    @test Optim.converged(r)
    @test Optim.g_converged(r)

    # Objective call counts are tracked and exposed.
    @test Optim.f_calls(r) > 0
    @test Optim.iterations(r) > 0

    # iterations option bounds the loop deterministically.
    rcap = optimize(f, [3.0, -1.0], NelderMead(), Optim.Options(iterations = 3))
    @test Optim.iterations(rcap) <= 3
end

true
