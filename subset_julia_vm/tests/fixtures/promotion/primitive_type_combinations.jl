# Comprehensive primitive type combination tests
# Verifies that mixed-type arithmetic operations produce correct values
# and that conversions between primitive types work without crashing.
# Prevention test for Issue #1659 / #1667

using Test

# Float32 mixed-type: values are correct
@testset "Float32 mixed-type values" begin
    @test Float32(2.0) + Int64(1) == 3.0
    @test Int64(1) + Float32(2.0) == 3.0
    @test Float32(5.0) - Int64(2) == 3.0
    @test Float32(2.0) * Int64(3) == 6.0
    @test Float32(6.0) / Int64(2) == 3.0
end

@testset "Float32 + Float64 promotion" begin
    @test typeof(Float32(2.0) + Float64(1.0)) == Float64
    @test typeof(Float64(1.0) + Float32(2.0)) == Float64
    @test Float32(2.0) + Float64(1.0) == 3.0
    @test Float32(5.0) - Float64(2.0) == 3.0
    @test Float32(2.0) * Float64(3.0) == 6.0
    @test Float32(6.0) / Float64(2.0) == 3.0
end

# Float16 mixed-type: values are correct
@testset "Float16 mixed-type values" begin
    @test Float16(2.0) + Int64(1) == Float16(3.0)
    @test typeof(Float16(2.0) + Int64(1)) == Float16
    @test Int64(1) + Float16(2.0) == Float16(3.0)
    @test Float16(5.0) - Int64(2) == Float16(3.0)
    @test Float16(2.0) * Int64(3) == Float16(6.0)
    @test Float16(6.0) / Int64(2) == Float16(3.0)
end

@testset "Float16 + Float64 promotion" begin
    @test typeof(Float16(2.0) + Float64(1.0)) == Float64
    @test typeof(Float64(1.0) + Float16(2.0)) == Float64
    @test Float16(2.0) + Float64(1.0) == 3.0
end

@testset "Float16 + Float32 promotion" begin
    @test typeof(Float16(2.0) + Float32(1.0)) == Float32
    @test typeof(Float32(1.0) + Float16(2.0)) == Float32
    @test Float16(2.0) + Float32(1.0) == Float32(3.0)
end

@testset "Float64 mixed-type values" begin
    @test Float64(2.0) + Int64(1) == 3.0
    @test typeof(Float64(2.0) + Int64(1)) == Float64
    @test Int64(1) + Float64(2.0) == 3.0
    @test Float64(5.0) - Int64(2) == 3.0
    @test Float64(2.0) * Int64(3) == 6.0
    @test Float64(6.0) / Int64(2) == 3.0
end

true
