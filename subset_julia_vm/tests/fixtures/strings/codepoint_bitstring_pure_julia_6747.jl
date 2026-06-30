# Issue #6747: codepoint and bitstring are now pure Julia. codepoint(c) = the
# Unicode codepoint as UInt32 (base/strings/basic.jl); bitstring(x) builds the
# binary representation from the bits via reinterpret-to-unsigned
# (base/intfuncs.jl). The raw-byte-access primitives ncodeunits / codeunit /
# codeunits and the Char(n)/Int(c) bit-constructors stay as Rust primitives per
# the issue's boundary policy. Values verified against upstream julia 1.12.

using Test

@testset "codepoint is pure Julia, returns UInt32 (Issue #6747)" begin
    @test codepoint('A') === UInt32(65)
    @test codepoint('z') === UInt32(122)
    @test codepoint('0') === UInt32(48)
    @test codepoint('é') === UInt32(0x00e9)
    @test map(codepoint, ['a', 'b']) == UInt32[97, 98]
end

@testset "bitstring is pure Julia, matches upstream (Issue #6747)" begin
    @test bitstring(Int32(4)) == "00000000000000000000000000000100"
    @test bitstring(UInt8(5)) == "00000101"
    @test bitstring(Int8(-1)) == "11111111"
    @test bitstring(UInt16(0xABCD)) == "1010101111001101"
    @test bitstring(Int64(-1)) == "1111111111111111111111111111111111111111111111111111111111111111"
    @test bitstring(1.0f0) == "00111111100000000000000000000000"
    @test bitstring(2.2) == "0100000000000001100110011001100110011001100110011001100110011010"
    @test bitstring(true) == "00000001"
    @test bitstring(Float16(1.0)) == "0011110000000000"
end

@testset "byte-access primitives still work (Issue #6747)" begin
    @test ncodeunits("abc") == 3
    @test codeunit("abc", 1) == 0x61
    @test Char(65) === 'A'
    @test Int('A') === 65
end

true
