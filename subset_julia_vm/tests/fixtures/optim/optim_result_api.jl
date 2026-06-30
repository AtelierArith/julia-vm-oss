# Optim.jl MVP — result/query API, Options, and maximize (Issue #7480).

using Test
using Optim

@testset "Optim result API and maximize" begin
    f = x -> (x - 2)^2

    r = optimize(f, -10.0, 10.0, Brent())

    # Public query API on a univariate result.
    @test abs(Optim.minimizer(r) - 2.0) < 1e-8
    @test Optim.minimum(r) < 1e-12
    @test Optim.iterations(r) >= 0
    @test Optim.converged(r)
    @test Optim.f_calls(r) > 0
    @test Optim.lower_bound(r) == -10.0
    @test Optim.upper_bound(r) == 10.0

    # Options for MVP keywords.
    o = Optim.Options(iterations = 50, show_trace = false, store_trace = false)
    @test o.iterations == 50
    r2 = optimize(f, -10.0, 10.0, GoldenSection(); iterations = 50)
    @test Optim.iterations(r2) <= 50

    # maximize wrapper minimizes the negated objective.
    m = maximize(x -> -(x - 2)^2, -10.0, 10.0)
    @test abs(Optim.maximizer(m) - 2.0) < 1e-6
    @test abs(Optim.maximum(m)) < 1e-6
    @test Optim.converged(m)
end

true
