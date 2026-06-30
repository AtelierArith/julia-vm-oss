# Issue #6722: bitrotate must preserve the element type across all
# BitInteger widths (it previously coerced to Int64 via the I64-only
# Rust handler, e.g. `bitrotate(UInt8(0b10110001), 2)` returned the
# Int64 value 708 instead of the UInt8 value 198).
#
# After migrating `bitrotate` to the upstream pure-Julia definition
#   bitrotate(x::T, k) where {T<:BitInteger} =
#       (x << ((sizeof(T)<<3 - 1) & k)) | (x >>> ((sizeof(T)<<3 - 1) & -k))
# the result type matches the input type for every integer width, and
# the rotation values match upstream `julia` (verified against 1.12).

using Test

@testset "bitrotate preserves element type (Issue #6722)" begin
    # `===` is type-strict: catches the Int64-coercion regression.
    @test bitrotate(UInt8(0b10110001), 2) === UInt8(198)
    @test bitrotate(Int8(-15), 2)         === Int8(-57)
    @test bitrotate(UInt16(0xF0F0), 2)    === UInt16(50115)
    @test bitrotate(Int16(-2000), 2)      === Int16(-7997)
    @test bitrotate(UInt32(0xDEADBEEF), 2) === UInt32(2058812351)
    @test bitrotate(Int32(123456), 2)     === Int32(493824)
    @test bitrotate(UInt64(0x0123456789ABCDEF), 2) === UInt64(327942116865947580)
    @test bitrotate(Int64(-987654321), 2) === Int64(-3950617281)
    @test bitrotate(UInt128(1) << 100, 2) === (UInt128(1) << 102)
end

@testset "bitrotate value parity with upstream (Issue #6722)" begin
    x8 = UInt8(0b10110001)               # 177
    @test bitrotate(x8, 0)   == 177
    @test bitrotate(x8, 1)   == 99
    @test bitrotate(x8, 7)   == 216
    @test bitrotate(x8, -3)  == 54
    @test bitrotate(x8, 64)  == 177       # k mod bitwidth wraps
    @test bitrotate(x8, 65)  == 99
    @test bitrotate(x8, -65) == 216

    # signed wrap (Int16)
    @test bitrotate(Int16(-2000), -65) === Int16(31768)

    # Int64 regression guard (the previously-supported path)
    @test bitrotate(Int64(-987654321), 1)  === Int64(-1975308641)
    @test bitrotate(1, 1)                   === 2
end

@testset "bit-op derived functions parity (Issue #6722)" begin
    # count_zeros / leading_ones / trailing_ones move to pure Julia but
    # must keep returning Int and matching upstream across widths.
    @test count_zeros(UInt8(0b1010)) === 6
    @test count_zeros(Int8(-1))      === 0
    @test count_zeros(Int64(0))      === 64
    @test leading_ones(0xf0)         === 4
    @test leading_ones(UInt8(0xff))  === 8
    @test trailing_ones(0b10111)     === 3
    @test trailing_ones(UInt8(0x0f)) === 4

    # first-class function values keep working after migration (Issue #5333)
    @test map(count_zeros, UInt8[0, 1, 255]) == [8, 7, 0]
    @test map(leading_ones, [-1, 0]) == [64, 0]
    @test map(trailing_ones, [7, 6]) == [3, 0]
    br = bitrotate
    @test br(UInt8(0b10110001), 2) === UInt8(198)
end

true
