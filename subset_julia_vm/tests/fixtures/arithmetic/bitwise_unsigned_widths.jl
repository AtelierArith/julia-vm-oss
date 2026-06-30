# Bitwise operators on unsigned and narrow integer widths (Issue #3565)
using Test

@testset "UInt8 bitwise" begin
    @test 0xff & 0x0f == 0x0f
    @test 0x12 | 0x34 == 0x36
    @test 0xab ⊻ 0x0f == 0xa4
    @test xor(0xab, 0x0f) == 0xa4
    @test ~UInt8(0) == 0xff
    @test ~UInt8(0xab) == 0x54
    @test 0xab << 1 == 0x56
    @test 0xab >> 4 == 0x0a
    @test 0xab >>> 4 == 0x0a

    # Result type is preserved
    @test typeof(UInt8(1) & UInt8(2)) == UInt8
    @test typeof(UInt8(1) | UInt8(2)) == UInt8
    @test typeof(UInt8(1) ⊻ UInt8(2)) == UInt8
    @test typeof(~UInt8(1)) == UInt8
    @test typeof(UInt8(1) << 1) == UInt8
    @test typeof(UInt8(0xab) >> 1) == UInt8
end

@testset "UInt16 bitwise" begin
    @test UInt16(0xff00) & UInt16(0x0ff0) == UInt16(0x0f00)
    @test UInt16(0xff00) | UInt16(0x0ff0) == UInt16(0xfff0)
    @test UInt16(0xff00) ⊻ UInt16(0x0ff0) == UInt16(0xf0f0)
    @test ~UInt16(0) == UInt16(0xffff)
    @test UInt16(1) << 8 == UInt16(0x0100)
    @test UInt16(0x0100) >> 8 == UInt16(0x0001)
    @test typeof(UInt16(1) & UInt16(2)) == UInt16
end

@testset "UInt32 bitwise" begin
    @test UInt32(0xffff0000) & UInt32(0x0ffff000) == UInt32(0x0fff0000)
    @test UInt32(0xffff0000) | UInt32(0x0000ffff) == UInt32(0xffffffff)
    @test ~UInt32(0) == UInt32(0xffffffff)
    @test UInt32(1) << 16 == UInt32(0x00010000)
    @test UInt32(0x00010000) >> 16 == UInt32(0x00000001)
    @test typeof(UInt32(1) & UInt32(2)) == UInt32
end

@testset "UInt64 bitwise" begin
    @test UInt64(0xff00ff00ff00ff00) & UInt64(0x0ff00ff00ff00ff0) == UInt64(0x0f000f000f000f00)
    @test UInt64(0xff00ff00ff00ff00) | UInt64(0x00ff00ff00ff00ff) == UInt64(0xffffffffffffffff)
    @test ~UInt64(0) == UInt64(0xffffffffffffffff)
    @test UInt64(1) << 32 == UInt64(0x100000000)
    @test UInt64(0x100000000) >> 32 == UInt64(1)
    @test typeof(UInt64(1) & UInt64(2)) == UInt64
end

@testset "Combined bitwise on UInt8 (bytes2hex-style)" begin
    v = 0xab
    hi = (v >> 4) & 0x0f
    lo = v & 0x0f
    @test hi == 0x0a
    @test lo == 0x0b
    @test typeof(hi) == UInt8
    @test typeof(lo) == UInt8
end

true
