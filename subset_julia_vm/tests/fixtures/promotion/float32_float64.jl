# Float32 × Float64 arithmetic tests
# Tests Float32 + Float64 combinations ONLY (promotes to Float64)
# See Issue #1661 for why tests are separated by type combination

using Test

@testset "Float32 + Float64 arithmetic (promotion)" begin
    @testset "Float32 + Float64" begin
        @test Float32(2.5) + 1.5 == 4.0
        @test typeof(Float32(2.5) + 1.5) == Float64
    end

    @testset "Float64 + Float32" begin
        @test 1.5 + Float32(2.5) == 4.0
        @test typeof(1.5 + Float32(2.5)) == Float64
    end

    @testset "Float32 - Float64" begin
        @test Float32(5.5) - 2.5 == 3.0
        @test typeof(Float32(5.5) - 2.5) == Float64
    end

    @testset "Float32 * Float64" begin
        @test Float32(2.5) * 2.0 == 5.0
        @test typeof(Float32(2.5) * 2.0) == Float64
    end

    @testset "Float32 / Float64" begin
        @test Float32(5.0) / 2.0 == 2.5
        @test typeof(Float32(5.0) / 2.0) == Float64
    end
end

true
