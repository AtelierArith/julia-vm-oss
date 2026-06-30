# Issue #4787: bswap for non-Int64 integer types returned wrong
# results because the compile path coerced the argument to I64 via
# `compile_expr_as(..., ValueType::I64)`. The runtime arm then
# byte-swapped all 8 bytes regardless of the original width.
# Same root cause family as #4785 — bit-op compile arms had a
# shared "force-to-I64" pattern; #4786 fixed count_zeros /
# leading_* / bitreverse but bswap was left behind.
#
# Fix: compile path uses plain compile_expr (preserves element
# type); runtime arm match-dispatches on the integer variant and
# pushes the original variant back.

using Test

@testset "bswap respects element bit width (Issue #4787)" begin
    @test bswap(UInt16(0x1234)) === UInt16(0x3412)
    @test bswap(UInt32(0x12345678)) === UInt32(0x78563412)
    @test bswap(UInt8(0x12)) === UInt8(0x12)     # single byte: unchanged
    @test bswap(UInt64(0x0102030405060708)) === UInt64(0x0807060504030201)
end

@testset "bswap signed integer types (Issue #4787)" begin
    @test bswap(Int16(0x1234)) === Int16(0x3412)
    @test bswap(Int32(0x12345678)) === Int32(0x78563412)
    @test bswap(Int8(0x12)) === Int8(0x12)
end

@testset "bswap Int64 regression guard (Issue #4787)" begin
    # The Int64 case must keep working unchanged
    @test bswap(Int64(0x0102030405060708)) === Int64(0x0807060504030201)
    @test bswap(UInt64(0)) === UInt64(0)
end

@testset "bswap preserves element type (Issue #4787)" begin
    @test typeof(bswap(UInt16(0x1234))) === UInt16
    @test typeof(bswap(UInt32(0x12345678))) === UInt32
    @test typeof(bswap(Int8(0x12))) === Int8
end

true
