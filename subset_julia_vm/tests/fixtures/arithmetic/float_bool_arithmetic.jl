using Test

@testset "Float32/Float16 + Bool arithmetic type preservation" begin
    # Float32 + Bool should return Float32 (Issue #2203)
    @test typeof(Float32(2.0) + true) == Float32
    @test typeof(true + Float32(2.0)) == Float32
    @test typeof(Float32(2.0) - true) == Float32
    @test typeof(Float32(2.0) * true) == Float32

    # Float16 + Bool should return Float16
    @test typeof(Float16(2.0) + true) == Float16
    @test typeof(true + Float16(2.0)) == Float16
    @test typeof(Float16(2.0) - true) == Float16
    @test typeof(Float16(2.0) * true) == Float16

    # Value correctness
    @test Float32(2.0) + true == Float32(3.0)
    @test Float32(2.0) - true == Float32(1.0)
    @test Float32(2.0) * true == Float32(2.0)
    @test Float32(2.0) * false == Float32(0.0)

    # Float64 + Bool should return Float64 (baseline)
    @test typeof(2.0 + true) == Float64
    @test typeof(2.0 * true) == Float64
end

true
