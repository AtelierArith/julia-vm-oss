# Test promote for Float32 value conversions (Issue #1772)
# Verifies that promote correctly converts values to common Float32 type

using Test

@testset "promote Float32 value conversions" begin
    @testset "Float32 with Int64" begin
        # promote(Float32, Int64) should return (Float32, Float32)
        result = promote(Float32(1.5), Int64(2))
        @test result[1] === Float32(1.5)
        @test result[2] === Float32(2.0)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Int64 with Float32" begin
        # promote(Int64, Float32) should return (Float32, Float32)
        result = promote(Int64(3), Float32(2.5))
        @test result[1] === Float32(3.0)
        @test result[2] === Float32(2.5)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Float32 with Bool" begin
        # promote(Float32, Bool) should return (Float32, Float32)
        result = promote(Float32(1.5), true)
        @test result[1] === Float32(1.5)
        @test result[2] === Float32(1.0)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32

        result2 = promote(false, Float32(2.5))
        @test result2[1] === Float32(0.0)
        @test result2[2] === Float32(2.5)
    end

    @testset "Float32 with Float64" begin
        # promote(Float32, Float64) should return (Float64, Float64)
        result = promote(Float32(1.5), 2.5)
        @test typeof(result[1]) === Float64
        @test typeof(result[2]) === Float64
    end
end

true
