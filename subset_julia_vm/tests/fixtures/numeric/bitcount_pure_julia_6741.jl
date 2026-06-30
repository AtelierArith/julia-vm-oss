# Issue #6741: the bit CPU functions count_ones / leading_zeros /
# trailing_zeros / bitreverse / bswap are now pure-Julia public functions
# (base/int.jl) that call the underscored low-level intrinsics
# _ctpop_int / _ctlz_int / _cttz_int / _bitreverse_int / _bswap_int. Only the
# CPU intrinsic remains on the Rust side (mirrors upstream
# count_ones(x) = ctpop_int(x) % Int). Behavior matches upstream julia 1.12
# across integer widths, and they keep working as first-class function values.

using Test

@testset "count_ones / leading_zeros / trailing_zeros (Issue #6741)" begin
    @test count_ones(11) === 3
    @test count_ones(UInt8(0xff)) === 8
    @test count_ones(Int8(-1)) === 8
    @test count_ones(0) === 0
    @test leading_zeros(UInt8(1)) === 7
    @test leading_zeros(Int64(1)) === 63
    @test leading_zeros(UInt16(0x0001)) === 15
    @test trailing_zeros(UInt8(0x80)) === 7
    @test trailing_zeros(8) === 3
    @test trailing_zeros(Int32(0)) === 32
end

@testset "bitreverse / bswap preserve element type (Issue #6741)" begin
    @test bitreverse(UInt8(0x01)) === UInt8(0x80)
    @test bitreverse(UInt16(0x0001)) === UInt16(0x8000)
    @test bitreverse(Int64(1)) === (Int64(1) << 63)
    @test bswap(UInt16(0x0102)) === UInt16(0x0201)
    @test bswap(UInt32(0x01020304)) === UInt32(0x04030201)
    @test bswap(Int16(0x0102)) === Int16(0x0201)
end

@testset "bit functions as first-class values (Issue #6741)" begin
    @test map(count_ones, [7, 8]) == [3, 1]
    @test map(leading_zeros, Int8[1, 1]) == [7, 7]
    @test map(trailing_zeros, [8, 16]) == [3, 4]
    f = count_ones
    @test f(11) == 3
    g = bswap
    @test g(UInt16(0x0102)) === UInt16(0x0201)
    # derived helpers (Issue #6722) still resolve through the new wrappers
    @test count_zeros(UInt8(0b1010)) === 6
    @test leading_ones(0xf0) === 4
end

true
