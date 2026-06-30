# Issue #4785: count_zeros, leading_zeros, leading_ones (and the
# rest of the bit-op family) for non-Int64 integer types returned
# wrong results because the compile path coerced the argument to
# I64 via `compile_expr_as(..., ValueType::I64)`. The runtime arm
# then operated on a 64-bit value, inflating zero counts due to
# the implicit zero-/sign-extension.
#
# Fix: compile path no longer coerces (uses plain `compile_expr`);
# runtime arms in vm/builtins_math.rs dispatch on the actual
# integer variant so the bit width is preserved.

using Test

@testset "count_zeros respects element bit width (Issue #4785)" begin
    # 0xf0 in 8 bits = 0b1111_0000 → 4 zero bits, not 56+4 = 60
    @test count_zeros(UInt8(0xf0)) == 4
    @test count_zeros(UInt8(0xff)) == 0
    @test count_zeros(UInt8(0x00)) == 8

    # 16-bit and 32-bit also need to respect width
    @test count_zeros(UInt16(0xff00)) == 8
    @test count_zeros(UInt32(0xffff_0000)) == 16

    # Int8 (signed): 0x7f = 0b0111_1111 → 1 zero (sign bit)
    @test count_zeros(Int8(127)) == 1

    # Int64 regression guard
    @test count_zeros(Int64(0)) == 64
end

@testset "leading_zeros respects element bit width (Issue #4785)" begin
    # 0x01 in 8 bits → 7 leading zeros, not 63
    @test leading_zeros(UInt8(0x01)) == 7
    @test leading_zeros(UInt8(0x80)) == 0
    @test leading_zeros(UInt16(0x0001)) == 15
    @test leading_zeros(UInt32(0x0000_0001)) == 31

    # Int8 (signed): 0x01 → 7 leading zeros
    @test leading_zeros(Int8(1)) == 7

    # Int64 regression guard
    @test leading_zeros(Int64(1)) == 63
end

@testset "leading_ones respects element bit width (Issue #4785)" begin
    # 0xf0 in 8 bits → 4 leading ones, not 0
    @test leading_ones(UInt8(0xf0)) == 4
    @test leading_ones(UInt8(0xff)) == 8
    @test leading_ones(UInt8(0x00)) == 0

    # 16-bit
    @test leading_ones(UInt16(0xff00)) == 8
end

@testset "count_ones / trailing_zeros / trailing_ones unchanged (Issue #4785)" begin
    # These already worked coincidentally because they count from the
    # LSB; regression guard.
    @test count_ones(UInt8(0xff)) == 8
    @test count_ones(UInt8(0x00)) == 0
    @test trailing_zeros(UInt8(0x80)) == 7
    @test trailing_ones(UInt8(0x0f)) == 4
end

@testset "bitreverse preserves element type (Issue #4785)" begin
    @test bitreverse(UInt8(0x01)) === UInt8(0x80)
    @test bitreverse(UInt16(0x0001)) === UInt16(0x8000)
    @test bitreverse(Int64(1)) === Int64(1) << 63
end

true
