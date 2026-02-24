# Float32 same-type arithmetic tests
# Tests Float32 × Float32 arithmetic ONLY (no cross-type operations)
# See Issue #1661 for why tests are separated by type combination

using Test

@testset "Float32 same-type arithmetic" begin
    @testset "Addition" begin
        @test Float32(2.5) + Float32(1.5) == Float32(4.0)
        @test typeof(Float32(2.5) + Float32(1.5)) == Float32
    end

    @testset "Subtraction" begin
        @test Float32(5.0) - Float32(2.0) == Float32(3.0)
        @test typeof(Float32(5.0) - Float32(2.0)) == Float32
    end

    @testset "Multiplication" begin
        @test Float32(2.5) * Float32(2.0) == Float32(5.0)
        @test typeof(Float32(2.5) * Float32(2.0)) == Float32
    end

    @testset "Division" begin
        @test Float32(5.0) / Float32(2.0) == Float32(2.5)
        @test typeof(Float32(5.0) / Float32(2.0)) == Float32
    end
end

true
