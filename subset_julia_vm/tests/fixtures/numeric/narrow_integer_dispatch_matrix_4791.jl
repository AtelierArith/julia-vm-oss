# Issue #4791: prevention matrix for the narrow-integer dispatch
# parity family (#4785 #4787 #4789). VM ops historically had arms
# for I64 only, silently wrong / crashing on narrower widths.
#
# This fixture is the CI-enforced sibling of the narrow-integer
# matrix section added to scripts/probe_base_api_parity.sh in the
# same PR. Each cell exercises a (numeric op, narrow integer width)
# combination; failure means a regression in the
# wrapping_neg / element-type-preserving dispatch added in PRs
# #4786 / #4788 / #4790 (or a new op missing narrow-width arms).

using Test

@testset "abs(typemin(IntN)) wraps to typemin for every signed width (Issue #4791)" begin
    @test abs(typemin(Int8)) === typemin(Int8)
    @test abs(typemin(Int16)) === typemin(Int16)
    @test abs(typemin(Int32)) === typemin(Int32)
    @test abs(typemin(Int64)) === typemin(Int64)
    @test abs(typemin(Int128)) === typemin(Int128)
end

@testset "unary -(typemin(IntN)) wraps to typemin (Issue #4791)" begin
    @test -typemin(Int8) === typemin(Int8)
    @test -typemin(Int16) === typemin(Int16)
    @test -typemin(Int32) === typemin(Int32)
    @test -typemin(Int64) === typemin(Int64)
    @test -typemin(Int128) === typemin(Int128)
end

@testset "count_zeros respects element bit width (Issue #4791)" begin
    @test count_zeros(UInt8(0xf0)) == 4
    @test count_zeros(UInt16(0xff00)) == 8
    @test count_zeros(UInt32(0xffff_0000)) == 16
    @test count_zeros(Int8(127)) == 1
    @test count_zeros(Int8(0)) == 8
    @test count_zeros(Int16(0)) == 16
    @test count_zeros(UInt8(0)) == 8
end

@testset "leading_zeros / leading_ones respect element bit width (Issue #4791)" begin
    @test leading_zeros(UInt8(0x01)) == 7
    @test leading_zeros(UInt16(0x0001)) == 15
    @test leading_zeros(UInt32(0x0000_0001)) == 31
    @test leading_ones(UInt8(0xf0)) == 4
    @test leading_ones(UInt16(0xff00)) == 8
    @test leading_ones(UInt8(0xff)) == 8
end

@testset "bswap preserves element type and width (Issue #4791)" begin
    @test bswap(UInt16(0x1234)) === UInt16(0x3412)
    @test bswap(UInt32(0x12345678)) === UInt32(0x78563412)
    @test bswap(UInt8(0x12)) === UInt8(0x12)
    @test typeof(bswap(UInt16(0x1234))) === UInt16
    @test typeof(bswap(UInt32(0x12345678))) === UInt32
    @test typeof(bswap(Int8(0x12))) === Int8
end

@testset "bitreverse preserves element type (Issue #4791)" begin
    @test typeof(bitreverse(UInt8(0x01))) === UInt8
    @test typeof(bitreverse(UInt16(0x0001))) === UInt16
    @test typeof(bitreverse(UInt32(0x0000_0001))) === UInt32
    @test bitreverse(UInt8(0x01)) === UInt8(0x80)
    @test bitreverse(UInt16(0x0001)) === UInt16(0x8000)
end

@testset "signbit / iseven / isodd on narrow integers (Issue #4791)" begin
    @test signbit(Int8(-1)) == true
    @test signbit(Int16(0)) == false
    @test iseven(Int8(2)) == true
    @test iseven(UInt8(3)) == false
    @test isodd(UInt8(3)) == true
    @test isodd(Int16(0)) == false
end

true
