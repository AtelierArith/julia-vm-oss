# Optim.jl BFGS — exact-parity quadratic cases (Issue #8059).
#
# For well-conditioned problems that BFGS solves in a single step the line-search
# secant lands exactly on the minimizer, so sjulia reproduces upstream Optim's
# minimizer, minimum, iteration count, and f/g-call counts *exactly*.
#
# (An objective bound to a variable literally named `f` works identically — the
# closure-capture name collision of Issue #8080 is fixed; see
# optim_objective_named_f.jl.) Verified identical under upstream Optim + sjulia.

using Test
using Optim

# Convex quadratic with analytic minimum at [1, 2], user gradient.
quadf(x) = (x[1] - 1.0)^2 + (x[2] - 2.0)^2
function quadg!(G, x)
    G[1] = 2.0 * (x[1] - 1.0)
    G[2] = 2.0 * (x[2] - 2.0)
    return G
end

# sum(abs2, x): minimum 0 at the origin, user gradient.
sumsq(x) = sum(abs2, x)
function sumsq_g!(G, x)
    for i in eachindex(x)
        G[i] = 2.0 * x[i]
    end
    return G
end

@testset "Optim BFGS quadratic (exact parity)" begin
    r = optimize(quadf, quadg!, [0.0, 0.0], BFGS())
    mz = Optim.minimizer(r)
    @test mz[1] == 1.0
    @test mz[2] == 2.0
    @test Optim.minimum(r) == 0.0
    @test Optim.iterations(r) == 1
    @test Optim.f_calls(r) == 3
    @test Optim.g_calls(r) == 3
    @test Optim.converged(r)
    @test Optim.g_converged(r)
end

@testset "Optim BFGS sum(abs2) (exact parity)" begin
    r = optimize(sumsq, sumsq_g!, [3.0, -1.0, 2.0], BFGS())
    mz = Optim.minimizer(r)
    @test mz[1] == 0.0
    @test mz[2] == 0.0
    @test mz[3] == 0.0
    @test Optim.minimum(r) == 0.0
    @test Optim.iterations(r) == 1
    @test Optim.f_calls(r) == 3
    @test Optim.g_calls(r) == 3
    @test Optim.converged(r)
end

true
