using Test

# Test that arithmetic on same-type small integers preserves bit width (Issue #2278).
# Previously, Int8(1) + Int8(2) returned Int64 instead of Int8.

@testset "Signed integer arithmetic preserves bit width" begin
    @test typeof(Int8(1) + Int8(2)) == Int8
    @test typeof(Int8(3) - Int8(1)) == Int8
    @test typeof(Int8(2) * Int8(3)) == Int8
    @test typeof(Int16(1) + Int16(2)) == Int16
    @test typeof(Int16(3) - Int16(1)) == Int16
    @test typeof(Int16(2) * Int16(3)) == Int16
    @test typeof(Int32(1) + Int32(2)) == Int32
    @test typeof(Int32(3) - Int32(1)) == Int32
    @test typeof(Int32(2) * Int32(3)) == Int32
end

@testset "Unsigned integer arithmetic preserves bit width" begin
    @test typeof(UInt8(1) + UInt8(2)) == UInt8
    @test typeof(UInt8(3) - UInt8(1)) == UInt8
    @test typeof(UInt8(2) * UInt8(3)) == UInt8
    @test typeof(UInt16(1) + UInt16(2)) == UInt16
    @test typeof(UInt16(3) - UInt16(1)) == UInt16
    @test typeof(UInt16(2) * UInt16(3)) == UInt16
    @test typeof(UInt32(1) + UInt32(2)) == UInt32
    @test typeof(UInt32(3) - UInt32(1)) == UInt32
    @test typeof(UInt32(2) * UInt32(3)) == UInt32
    @test typeof(UInt64(1) + UInt64(2)) == UInt64
    @test typeof(UInt64(3) - UInt64(1)) == UInt64
    @test typeof(UInt64(2) * UInt64(3)) == UInt64
end

@testset "Float16/Float32 arithmetic preserves bit width" begin
    @test typeof(Float16(1.0) + Float16(2.0)) == Float16
    @test typeof(Float16(3.0) - Float16(1.0)) == Float16
    @test typeof(Float16(2.0) * Float16(3.0)) == Float16
    @test typeof(Float32(1.0) + Float32(2.0)) == Float32
    @test typeof(Float32(3.0) - Float32(1.0)) == Float32
    @test typeof(Float32(2.0) * Float32(3.0)) == Float32
end

true
