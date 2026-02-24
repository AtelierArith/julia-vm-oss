# Float32 power (^) operations (Issue #1762)
# Tests Float32 power with type preservation for same-type and mixed-type operands

using Test

@testset "Float32 power operations" begin
    @testset "Float32 ^ Float32" begin
        @test Float32(2.0) ^ Float32(2.0) == Float32(4.0)
        @test typeof(Float32(2.0) ^ Float32(2.0)) == Float32
        @test Float32(3.0) ^ Float32(2.0) == Float32(9.0)
        @test typeof(Float32(3.0) ^ Float32(2.0)) == Float32
        @test Float32(4.0) ^ Float32(0.5) == Float32(2.0)
        @test typeof(Float32(4.0) ^ Float32(0.5)) == Float32
    end

    @testset "Float32 ^ Int" begin
        @test Float32(2.0) ^ 2 == Float32(4.0)
        @test typeof(Float32(2.0) ^ 2) == Float32
        @test Float32(2.0) ^ 3 == Float32(8.0)
        @test typeof(Float32(2.0) ^ 3) == Float32
        @test Float32(3.0) ^ 0 == Float32(1.0)
        @test typeof(Float32(3.0) ^ 0) == Float32
    end

    @testset "Int ^ Float32" begin
        @test 2 ^ Float32(3.0) == Float32(8.0)
        @test typeof(2 ^ Float32(3.0)) == Float32
        @test 3 ^ Float32(2.0) == Float32(9.0)
        @test typeof(3 ^ Float32(2.0)) == Float32
    end
end

true
