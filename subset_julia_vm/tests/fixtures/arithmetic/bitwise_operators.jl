# Test bit-shift and bitwise operators (Issue #2618)
using Test

@testset "Bit-shift operators" begin
    # Left shift: <<
    @test 1 << 0 == 1
    @test 1 << 1 == 2
    @test 1 << 3 == 8
    @test 5 << 2 == 20
    @test 1 << 10 == 1024

    # Arithmetic right shift: >> (preserves sign)
    @test 8 >> 1 == 4
    @test 8 >> 2 == 2
    @test 8 >> 3 == 1
    @test 1024 >> 10 == 1
    @test -8 >> 1 == -4
    @test -1 >> 1 == -1

    # Logical right shift: >>> (fills with zeros)
    @test 8 >>> 1 == 4
    @test 8 >>> 2 == 2
    @test 8 >>> 3 == 1
end

@testset "Bitwise operators" begin
    # AND: &
    @test 0b1100 & 0b1010 == 0b1000
    @test 15 & 9 == 9
    @test 0 & 255 == 0
    @test 255 & 255 == 255

    # OR: |
    @test 0b1100 | 0b1010 == 0b1110
    @test 12 | 10 == 14
    @test 0 | 255 == 255
    @test 0 | 0 == 0

    # XOR: ⊻
    @test 0b1100 ⊻ 0b1010 == 0b0110
    @test 12 ⊻ 10 == 6
    @test 255 ⊻ 255 == 0
    @test 0 ⊻ 255 == 255

    # NOT: ~
    @test ~0 == -1
    @test ~1 == -2
    @test ~(-1) == 0
end

@testset "Combined bit operations" begin
    # Extracting bits with shift and mask
    x = 0b11010110
    @test (x >> 4) & 0x0f == 0b1101
    @test x & 0x0f == 0b0110

    # Setting bits
    @test 0 | (1 << 3) == 8

    # bytes2hex-style extraction (the original motivation)
    v = 0xab
    @test div(v, 16) == (v >> 4)
    @test v % 16 == (v & 0x0f)
end

true
