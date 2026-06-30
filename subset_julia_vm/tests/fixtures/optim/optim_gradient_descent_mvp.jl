# Optim.jl MVP — first-order user-gradient GradientDescent (Issue #7482).

using Test
using Optim

@testset "Optim GradientDescent MVP" begin
    # Convex quadratic with analytic minimum at [1, 2].
    f = x -> (x[1] - 1.0)^2 + (x[2] - 2.0)^2
    function g!(G, x)
        G[1] = 2.0 * (x[1] - 1.0)
        G[2] = 2.0 * (x[2] - 2.0)
        return G
    end

    r = optimize(f, g!, [0.0, 0.0], GradientDescent())

    mz = Optim.minimizer(r)
    @test abs(mz[1] - 1.0) < 1e-6
    @test abs(mz[2] - 2.0) < 1e-6
    @test Optim.minimum(r) < 1e-10
    @test Optim.converged(r)

    # User-gradient call counts are tracked.
    @test Optim.f_calls(r) > 0
    @test Optim.g_calls(r) > 0
    @test Optim.iterations(r) > 0
end

true
