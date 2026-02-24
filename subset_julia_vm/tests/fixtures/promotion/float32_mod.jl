# Float32 mod/rem operations test (Issue #1776, #1778 prevention)
# Tests that mod and rem operations work correctly with Float32 mixed types
# This prevents regressions like the SremInt error in circshift

using Test

@testset "Float32 mod operations" begin
    @testset "Float32 % Int64" begin
        # Float32 mod Int64 should return Float32
        result = Float32(5.0) % 3
        @test result == Float32(2.0)

        result = Float32(7.5) % 2
        @test result == Float32(1.5)
    end

    @testset "Int64 % Float32" begin
        # Int64 mod Float32 should return Float32
        result = 5 % Float32(3.0)
        @test result == Float32(2.0)

        result = 7 % Float32(2.5)
        @test result == Float32(2.0)
    end

    @testset "Float32 % Float32" begin
        # Both Float32 should return Float32
        result = Float32(5.0) % Float32(3.0)
        @test result == Float32(2.0)
    end

    @testset "Float32 % Float64" begin
        # Float32 mod Float64 should promote to Float64
        result = Float32(5.0) % 3.0
        @test result == 2.0
    end

    @testset "mod function with Float32" begin
        # mod function (not just % operator)
        @test mod(Float32(5.0), 3) == Float32(2.0)
        @test mod(5, Float32(3.0)) == Float32(2.0)
        @test mod(Float32(-5.0), 3) == Float32(1.0)  # mod handles negative differently than rem
    end
end

true
