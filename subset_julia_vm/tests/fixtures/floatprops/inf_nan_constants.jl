# Test Inf32, NaN32, Inf64, NaN64 constants

using Test

@testset "Float64 special constants" begin
    @test isinf(Inf)
    @test isinf(Inf64)
    @test Inf == Inf64
    @test isnan(NaN)
    @test isnan(NaN64)
    @test typeof(Inf) == Float64
    @test typeof(NaN) == Float64
    @test typeof(Inf64) == Float64
    @test typeof(NaN64) == Float64
end

@testset "Float32 special constants exist with correct type" begin
    # Note: Float32 arithmetic operators not fully implemented yet,
    # so we test only type correctness
    @test typeof(Inf32) == Float32
    @test typeof(NaN32) == Float32
end

true  # Test passed
