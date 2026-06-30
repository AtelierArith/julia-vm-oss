# Optim.jl BFGS — Rosenbrock, user-gradient and finite-difference (Issue #8059).
#
# The Rosenbrock valley is the canonical multi-step BFGS test. Both sjulia and
# upstream Optim drive the iterate to the minimizer [1, 1] with a near-zero
# objective, but the exact iteration / f-call / g-call counts are NOT asserted:
# they depend on the line-search internals and the floating-point reduction order
# of the inverse-Hessian `dot`/`mul!` (upstream uses BLAS; the no-JIT VM uses
# scalar loops), and they also drift across Optim releases (e.g. installed Optim
# 2.2.1 takes 21 user-gradient iterations vs the classic ~16). The
# finite-difference form likewise reaches [1, 1] on both, though upstream's
# internal `converged` flag is configuration/Optim-version dependent for the noisy
# finite-difference gradient — so only the converged solution quality is asserted.
#
# Assertions are deliberately limited to what holds IDENTICALLY under upstream
# Optim 2.2.1 (julia 1.12.6) and sjulia: convergence to the minimizer/minimum
# within solver tolerance, and the f_calls == g_calls invariant.

using Test
using Optim

rosenbrock(x) = (1.0 - x[1])^2 + 100.0 * (x[2] - x[1]^2)^2
function rosenbrock_g!(G, x)
    G[1] = -2.0 * (1.0 - x[1]) - 400.0 * x[1] * (x[2] - x[1]^2)
    G[2] = 200.0 * (x[2] - x[1]^2)
    return G
end

@testset "Optim BFGS Rosenbrock (user gradient)" begin
    r = optimize(rosenbrock, rosenbrock_g!, [0.0, 0.0], BFGS())
    mz = Optim.minimizer(r)
    @test abs(mz[1] - 1.0) < 1e-6
    @test abs(mz[2] - 1.0) < 1e-6
    @test Optim.minimum(r) < 1e-10
    @test Optim.converged(r)
    @test Optim.g_converged(r)
    @test Optim.f_calls(r) == Optim.g_calls(r)
    @test Optim.f_calls(r) > 0
end

@testset "Optim BFGS Rosenbrock (finite-difference gradient)" begin
    r = optimize(rosenbrock, [0.0, 0.0], BFGS())
    mz = Optim.minimizer(r)
    @test abs(mz[1] - 1.0) < 1e-6
    @test abs(mz[2] - 1.0) < 1e-6
    @test Optim.minimum(r) < 1e-10
    @test Optim.f_calls(r) == Optim.g_calls(r)
end

@testset "Optim BFGS maximize (concave quadratic)" begin
    concave(x) = -((x[1] - 3.0)^2 + (x[2] + 1.0)^2)
    mr = maximize(concave, [0.0, 0.0], BFGS())
    mz = Optim.maximizer(mr)
    @test abs(mz[1] - 3.0) < 1e-6
    @test abs(mz[2] + 1.0) < 1e-6
    @test Optim.maximum(mr) > -1e-10
    @test Optim.converged(mr)
end

true
