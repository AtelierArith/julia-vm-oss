# Float arithmetic type preservation test
# Ensures that arithmetic operations (+, -, *, /) preserve float types (F16, F32, F64)
# Prevention test for Issue #1647 / #1653

using Test

@testset "Float16 arithmetic type preservation" begin
    @test typeof(Float16(1.0) + Float16(2.0)) === Float16
    @test typeof(Float16(1.0) - Float16(2.0)) === Float16
    @test typeof(Float16(1.0) * Float16(2.0)) === Float16
    @test typeof(Float16(1.0) / Float16(2.0)) === Float16
end

@testset "Float32 arithmetic type preservation" begin
    @test typeof(Float32(1.0) + Float32(2.0)) === Float32
    @test typeof(Float32(1.0) - Float32(2.0)) === Float32
    @test typeof(Float32(1.0) * Float32(2.0)) === Float32
    @test typeof(Float32(1.0) / Float32(2.0)) === Float32
end

@testset "Float64 arithmetic type preservation" begin
    @test typeof(Float64(1.0) + Float64(2.0)) === Float64
    @test typeof(Float64(1.0) - Float64(2.0)) === Float64
    @test typeof(Float64(1.0) * Float64(2.0)) === Float64
    @test typeof(Float64(1.0) / Float64(2.0)) === Float64
end

true
