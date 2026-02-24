using Test

@testset "Float32/Float16 + small integer types type preservation" begin
    # Float32 + Int8 should return Float32 (Issue #2225)
    @test typeof(Float32(1.0) + Int8(1)) == Float32
    @test typeof(Float32(1.0) - Int8(1)) == Float32
    @test typeof(Float32(1.0) * Int8(2)) == Float32

    # Float32 + UInt8 should return Float32
    @test typeof(Float32(1.0) + UInt8(1)) == Float32
    @test typeof(Float32(1.0) - UInt8(1)) == Float32
    @test typeof(Float32(1.0) * UInt8(2)) == Float32

    # Float16 + Int8 should return Float16
    @test typeof(Float16(1.0) + Int8(1)) == Float16
    @test typeof(Float16(1.0) - Int8(1)) == Float16
    @test typeof(Float16(1.0) * Int8(2)) == Float16

    # Float16 + UInt8 should return Float16
    @test typeof(Float16(1.0) + UInt8(1)) == Float16
    @test typeof(Float16(1.0) - UInt8(1)) == Float16
    @test typeof(Float16(1.0) * UInt8(2)) == Float16

    # Value correctness
    @test Float32(1.0) + Int8(1) == Float32(2.0)
    @test Float32(1.0) * UInt8(3) == Float32(3.0)
    @test Float16(1.0) + Int8(1) == Float16(2.0)
    @test Float16(1.0) * UInt8(3) == Float16(3.0)
end

true
