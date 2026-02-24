using Test

# Test that numeric type constructors preserve bit width (Issue #1663).
# Previously, the type inference mapped smaller types (Int8/Int16/Int32, Float32)
# to larger types (Int64, Float64), losing type precision.

@testset "Integer constructor bit width" begin
    @test typeof(Int8(1)) == Int8
    @test typeof(Int16(1)) == Int16
    @test typeof(Int32(1)) == Int32
    @test typeof(Int64(1)) == Int64
end

@testset "Unsigned integer constructor bit width" begin
    @test typeof(UInt8(1)) == UInt8
    @test typeof(UInt16(1)) == UInt16
    @test typeof(UInt32(1)) == UInt32
    @test typeof(UInt64(1)) == UInt64
end

@testset "Float constructor bit width" begin
    @test typeof(Float16(1.0)) == Float16
    @test typeof(Float32(1.0)) == Float32
    @test typeof(Float64(1.0)) == Float64
end

@testset "Bool constructor" begin
    @test typeof(true) == Bool
    @test typeof(false) == Bool
end

true
