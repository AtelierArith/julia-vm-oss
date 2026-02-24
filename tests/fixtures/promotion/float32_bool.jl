# Float32 × Bool arithmetic tests
# Tests Float32 + Bool combinations ONLY
# See Issue #1661 for why tests are separated by type combination
# See Issue #1759 for the Float32 + Bool bug

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
