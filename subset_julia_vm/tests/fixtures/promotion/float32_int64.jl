# Float32 × Int64 arithmetic tests
# Tests Float32 + Int64 combinations ONLY
# See Issue #1661 for why tests are separated by type combination

using Test

@testset "Float32 + Int64 arithmetic" begin
    @testset "Float32 + Int64" begin
        @test Float32(2.5) + 3 == Float32(5.5)
        @test typeof(Float32(2.5) + 3) == Float32
    end

    @testset "Int64 + Float32" begin
        @test 3 + Float32(2.5) == Float32(5.5)
        @test typeof(3 + Float32(2.5)) == Float32
    end

    @testset "Float32 - Int64" begin
        @test Float32(5.5) - 3 == Float32(2.5)
        @test typeof(Float32(5.5) - 3) == Float32
    end

    @testset "Float32 * Int64" begin
        @test Float32(2.5) * 2 == Float32(5.0)
        @test typeof(Float32(2.5) * 2) == Float32
    end

    @testset "Float32 / Int64" begin
        @test Float32(5.0) / 2 == Float32(2.5)
        @test typeof(Float32(5.0) / 2) == Float32
    end
end

true
