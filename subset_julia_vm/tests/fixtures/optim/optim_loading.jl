# Optim.jl MVP — package loading and minimal dependency resolution (Issue #7478).

using Test
using Optim
using NLSolversBase
using ADTypes
using LineSearches

@testset "Optim and dependencies load" begin
    # Optim exports the solver and configuration types.
    @test GoldenSection() isa Optim.UnivariateOptimizer
    @test Brent() isa Optim.UnivariateOptimizer
    @test NelderMead() isa Optim.AbstractOptimizer
    @test GradientDescent() isa Optim.AbstractOptimizer

    # NLSolversBase objective wrappers resolve.
    d = NonDifferentiable(x -> x[1]^2, [0.0])
    @test value(d, [3.0]) == 9.0
    @test f_calls(d) == 1

    # LineSearches and ADTypes resolve enough for MVP source loading.
    @test BackTracking() isa BackTracking
    @test AutoForwardDiff() isa ADTypes.AbstractADType
end

true
