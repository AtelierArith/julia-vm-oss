# Test Float16 mixed-type arithmetic dispatch
# Issue #1898: F16+I64, F16+F32, F16+F64 dispatch paths were missing

using Test

@testset "Float16 mixed-type arithmetic" begin
    # F16 + I64 -> F16
    @test Float16(2.5) + 1 == Float16(3.5)
    @test typeof(Float16(2.5) + 1) == Float16
    @test 1 + Float16(2.5) == Float16(3.5)
    @test typeof(1 + Float16(2.5)) == Float16

    # F16 - I64 -> F16
    @test Float16(5.0) - 2 == Float16(3.0)
    @test typeof(Float16(5.0) - 2) == Float16

    # F16 * I64 -> F16
    @test Float16(2.0) * 3 == Float16(6.0)
    @test typeof(Float16(2.0) * 3) == Float16

    # F16 / I64 -> F16
    @test Float16(6.0) / 2 == Float16(3.0)
    @test typeof(Float16(6.0) / 2) == Float16

    # F16 comparison with I64
    @test Float16(2.5) > 2
    @test Float16(2.5) < 3
    @test Float16(2.0) == 2
    @test Float16(2.5) != 2
    @test Float16(2.0) >= 2
    @test Float16(2.0) <= 2
end

@testset "Float16-Float64 promotion" begin
    # F16 + F64 -> F64
    @test Float16(2.5) + 1.0 == 3.5
    @test typeof(Float16(2.5) + 1.0) == Float64

    # F16 - F64 -> F64
    @test Float16(5.0) - 2.0 == 3.0
    @test typeof(Float16(5.0) - 2.0) == Float64

    # F16 * F64 -> F64
    @test Float16(2.0) * 3.0 == 6.0
    @test typeof(Float16(2.0) * 3.0) == Float64

    # F16 / F64 -> F64
    @test Float16(6.0) / 2.0 == 3.0
    @test typeof(Float16(6.0) / 2.0) == Float64
end

@testset "Float16-Float32 promotion" begin
    # F16 + F32 -> F32
    @test Float16(2.5) + Float32(1.0) == Float32(3.5)
    @test typeof(Float16(2.5) + Float32(1.0)) == Float32

    # F16 * F32 -> F32
    @test Float16(2.0) * Float32(3.0) == Float32(6.0)
    @test typeof(Float16(2.0) * Float32(3.0)) == Float32
end

true
