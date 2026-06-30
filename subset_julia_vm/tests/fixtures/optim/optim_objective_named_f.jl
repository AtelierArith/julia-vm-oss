# Optim.jl BFGS — objective bound to a variable literally named `f` (Issue #8080).
#
# Regression for the capture bug behind W-42: passing an objective bound to a
# variable named `f` to `optimize(f, ...)` (whose own objective parameter is also
# named `f`) used to trip a closure-capture name collision in the BFGS path —
# either directly or because the finite-difference gradient is built by a closure
# factory `_central_difference_gradient(f)` that captures `f`. Renaming the
# variable (`myf`) or using a named `function` avoided it. The underlying capture
# misresolution is fixed (W-42 removed); the name no longer matters.
#
# Asserted quantities hold IDENTICALLY under upstream Optim 2.2.1 (julia 1.12.6)
# and sjulia: the explicit-gradient case reproduces the named-function counts
# exactly; the finite-difference case asserts only solution quality and the
# f_calls == g_calls invariant (line-search internals / FD noise are version- and
# reduction-order-dependent — see optim_bfgs_rosenbrock.jl).

using Test
using Optim

@testset "Optim BFGS objective named `f` (explicit gradient)" begin
    # Objective and gradient both bound to variables named `f`/`g!`.
    f = x -> (x[1] - 1.0)^2 + (x[2] - 2.0)^2
    g! = function (G, x)
        G[1] = 2.0 * (x[1] - 1.0)
        G[2] = 2.0 * (x[2] - 2.0)
        return G
    end
    r = optimize(f, g!, [0.0, 0.0], BFGS())
    mz = Optim.minimizer(r)
    @test mz[1] == 1.0
    @test mz[2] == 2.0
    @test Optim.minimum(r) == 0.0
    @test Optim.iterations(r) == 1
    @test Optim.f_calls(r) == 3
    @test Optim.g_calls(r) == 3
    @test Optim.converged(r)
end

@testset "Optim BFGS objective named `f` (finite-difference closure factory)" begin
    # No user gradient -> NLSolversBase builds the gradient via the closure
    # factory `_central_difference_gradient(f)`, exercising the captured `f`.
    f = x -> (x[1] - 1.0)^2 + (x[2] - 2.0)^2
    r = optimize(f, [0.0, 0.0], BFGS())
    mz = Optim.minimizer(r)
    @test abs(mz[1] - 1.0) < 1e-6
    @test abs(mz[2] - 2.0) < 1e-6
    @test Optim.minimum(r) < 1e-10
    @test Optim.f_calls(r) == Optim.g_calls(r)
end

@testset "Optim GradientDescent objective named `f`" begin
    f = x -> (x[1] - 1.0)^2 + (x[2] - 2.0)^2
    g! = function (G, x)
        G[1] = 2.0 * (x[1] - 1.0)
        G[2] = 2.0 * (x[2] - 2.0)
        return G
    end
    r = optimize(f, g!, [0.0, 0.0], GradientDescent())
    mz = Optim.minimizer(r)
    @test abs(mz[1] - 1.0) < 1e-6
    @test abs(mz[2] - 2.0) < 1e-6
    @test Optim.converged(r)
end

true
