# Test reciprocal hyperbolic functions: sech, csch, coth (Issue #1863)

using Test

@testset "sech basic" begin
    @test sech(0.0) == 1.0
    @test sech(1.0) ≈ 1.0 / cosh(1.0) atol=1e-15
end

@testset "csch basic" begin
    @test csch(1.0) ≈ 1.0 / sinh(1.0) atol=1e-15
end

@testset "coth basic" begin
    @test coth(1.0) ≈ cosh(1.0) / sinh(1.0) atol=1e-15
end

true
