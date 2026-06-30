# Optim.jl MVP — bounded univariate minimization (Issue #7479).
# GoldenSection and Brent on f(x) = (x - 2)^2 over [-10, 10].
# Uses qualified `Optim.<query>` accessors so the fixture runs identically under
# upstream Optim.jl (which does not export the query API) and sjulia.

using Test
using Optim

@testset "Optim univariate GoldenSection / Brent" begin
    f = x -> (x - 2)^2

    # GoldenSection
    rg = optimize(f, -10.0, 10.0, GoldenSection())
    @test abs(Optim.minimizer(rg) - 2.0) < 1e-6
    @test Optim.minimum(rg) < 1e-12
    @test Optim.converged(rg)
    @test Optim.f_calls(rg) > 0
    @test Optim.iterations(rg) > 0

    # Brent
    rb = optimize(f, -10.0, 10.0, Brent())
    @test abs(Optim.minimizer(rb) - 2.0) < 1e-8
    @test Optim.minimum(rb) < 1e-12
    @test Optim.converged(rb)
    @test Optim.f_calls(rb) > 0

    # Integer bounds promote like upstream (default method = Brent).
    ri = optimize(f, -10, 10)
    @test abs(Optim.minimizer(ri) - 2.0) < 1e-8
    @test Optim.minimum(ri) < 1e-12

    # x_lower > x_upper raises a precise error.
    threw = false
    msg = ""
    try
        optimize(f, 10.0, -10.0, Brent())
    catch e
        threw = true
        msg = sprint(showerror, e)
    end
    @test threw
    @test occursin("x_lower must be less than x_upper", msg)
end

true
