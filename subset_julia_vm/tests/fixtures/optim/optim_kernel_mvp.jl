# Optim.jl MVP — representative end-to-end kernel (Issue #7483).
# Exercises univariate (Brent), derivative-free (NelderMead), and first-order
# (GradientDescent) solvers plus the maximize wrapper in one program.

using Test
using Optim

@testset "Optim MVP kernel" begin
    # Univariate Brent
    rb = optimize(x -> (x - 3.0)^2 + 1.0, -5.0, 5.0, Brent())
    @test abs(Optim.minimizer(rb) - 3.0) < 1e-6
    @test abs(Optim.minimum(rb) - 1.0) < 1e-9

    # Derivative-free NelderMead on a 2-D quadratic
    rn = optimize(x -> (x[1] - 1.0)^2 + (x[2] + 2.0)^2, [0.0, 0.0], NelderMead())
    mz = Optim.minimizer(rn)
    @test abs(mz[1] - 1.0) < 1e-2
    @test abs(mz[2] + 2.0) < 1e-2
    @test Optim.converged(rn)

    # First-order GradientDescent with user gradient
    f = x -> (x[1] - 4.0)^2 + (x[2] - 5.0)^2
    g! = (G, x) -> (G[1] = 2.0 * (x[1] - 4.0); G[2] = 2.0 * (x[2] - 5.0); G)
    rg = optimize(f, g!, [0.0, 0.0], GradientDescent())
    @test abs(Optim.minimizer(rg)[1] - 4.0) < 1e-6
    @test abs(Optim.minimizer(rg)[2] - 5.0) < 1e-6
    @test Optim.converged(rg)

    # maximize wrapper
    m = maximize(x -> -(x - 1.0)^2, -5.0, 5.0)
    @test abs(Optim.maximizer(m) - 1.0) < 1e-6
end

true
