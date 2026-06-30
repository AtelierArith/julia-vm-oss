using Random
using Test

function qualified_xoshiro_5436()
    rng = Random.Xoshiro(123)
    return rng !== nothing
end

function randn_rng_returns_float_5436()
    rng = Xoshiro(123)
    x = randn(rng)
    return x isa Float64
end

function qualified_randn_rng_returns_float_5436()
    rng = Random.Xoshiro(123)
    x = Random.randn(rng)
    return x isa Float64
end

@testset "Random.Xoshiro and randn(rng) (Issue #5436)" begin
    @test qualified_xoshiro_5436()
    @test randn_rng_returns_float_5436()
    @test qualified_randn_rng_returns_float_5436()
end

true
