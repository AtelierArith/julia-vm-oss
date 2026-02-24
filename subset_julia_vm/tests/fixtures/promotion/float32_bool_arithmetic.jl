# Float32 × Bool arithmetic tests (Issue #1759)
# Tests that Float32 + Bool returns Float32, not Float64

using Test

@testset "Float32 + Bool arithmetic" begin
    @testset "Float32 + true" begin
        @test Float32(2.5) + true == Float32(3.5)
        @test typeof(Float32(2.5) + true) == Float32
    end

    @testset "Float32 + false" begin
        @test Float32(2.5) + false == Float32(2.5)
        @test typeof(Float32(2.5) + false) == Float32
    end

    @testset "true + Float32" begin
        @test true + Float32(2.5) == Float32(3.5)
        @test typeof(true + Float32(2.5)) == Float32
    end

    @testset "Float32 * Bool" begin
        @test Float32(2.5) * true == Float32(2.5)
        @test Float32(2.5) * false == Float32(0.0)
        @test typeof(Float32(2.5) * true) == Float32
    end

    @testset "Float32 - Bool" begin
        @test Float32(3.5) - true == Float32(2.5)
        @test typeof(Float32(3.5) - true) == Float32
    end
end

true
