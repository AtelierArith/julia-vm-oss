# Test sinh(), cosh(), tanh() and inverse hyperbolic functions (Issue #1861)

using Test

@testset "sinh basic" begin
    @test sinh(0.0) == 0.0
    @test sinh(1.0) ≈ (exp(1.0) - exp(-1.0)) / 2.0 atol=1e-15
end

@testset "cosh basic" begin
    @test cosh(0.0) == 1.0
    @test cosh(1.0) ≈ (exp(1.0) + exp(-1.0)) / 2.0 atol=1e-15
end

@testset "tanh basic" begin
    @test tanh(0.0) == 0.0
    @test tanh(1.0) ≈ sinh(1.0) / cosh(1.0) atol=1e-15
end

@testset "asinh basic" begin
    @test asinh(0.0) == 0.0
    @test asinh(sinh(1.0)) ≈ 1.0 atol=1e-15
end

@testset "acosh basic" begin
    @test acosh(1.0) == 0.0
    @test acosh(cosh(1.0)) ≈ 1.0 atol=1e-15
end

@testset "atanh basic" begin
    @test atanh(0.0) == 0.0
    @test atanh(tanh(0.5)) ≈ 0.5 atol=1e-15
end

true
