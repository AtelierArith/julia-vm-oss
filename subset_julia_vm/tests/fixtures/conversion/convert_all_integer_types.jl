# Test convert() for all integer target types (Issue #2267)
# Verifies that convert(T, x) works for Int8-Int128, UInt8-UInt128 targets.

using Test

@testset "convert to Int8/Int16/Int32" begin
    @test convert(Int8, 42) == Int8(42)
    @test convert(Int8, 1.0) == Int8(1)
    @test typeof(convert(Int8, 42)) == Int8

    @test convert(Int16, 1000) == Int16(1000)
    @test convert(Int16, 1.0) == Int16(1)
    @test typeof(convert(Int16, 1000)) == Int16

    @test convert(Int32, 100000) == Int32(100000)
    @test convert(Int32, 1.0) == Int32(1)
    @test typeof(convert(Int32, 100000)) == Int32
end

@testset "convert to UInt8/UInt16/UInt32/UInt64" begin
    @test convert(UInt8, 255) == UInt8(255)
    @test typeof(convert(UInt8, 255)) == UInt8

    @test convert(UInt16, 65535) == UInt16(65535)
    @test typeof(convert(UInt16, 65535)) == UInt16

    @test convert(UInt32, 100000) == UInt32(100000)
    @test typeof(convert(UInt32, 100000)) == UInt32

    @test convert(UInt64, 1) == UInt64(1)
    @test typeof(convert(UInt64, 1)) == UInt64
end

@testset "convert to Int128/UInt128" begin
    @test convert(Int128, 42) == Int128(42)
    @test typeof(convert(Int128, 42)) == Int128

    @test convert(UInt128, 42) == UInt128(42)
    @test typeof(convert(UInt128, 42)) == UInt128
end

@testset "cross-type integer conversion" begin
    # Small to large
    @test convert(Int64, Int8(42)) == 42
    @test convert(Int128, Int32(100)) == Int128(100)

    # Large to small (truncation)
    @test convert(Int8, Int64(42)) == Int8(42)
    @test convert(UInt8, UInt64(200)) == UInt8(200)

    # Signed to unsigned
    @test convert(UInt32, Int64(100)) == UInt32(100)

    # Float to integer
    @test convert(Int8, 1.0) == Int8(1)
    @test convert(UInt16, 2.0) == UInt16(2)
end

true
