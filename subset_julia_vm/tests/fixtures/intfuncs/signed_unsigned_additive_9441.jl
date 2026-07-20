# Mixed Signed×Unsigned +, -, * return the UNSIGNED result type (Issue #9441)
#
# #9337 fixed the div/fld/cld/rem/mod result types, but +, -, * still returned
# the *signed* type for a same-width Signed×Unsigned pair. Upstream promotes to
# UInt (unsigned wins: promote_type(Int128, UInt128) == UInt128) and converts
# the signed operand via `x % UInt128` — a sign-extend + reinterpret, NOT a
# checked convert (which would InexactError on a negative). The result wraps in
# the unsigned type. Before the fix, typeof(Int128(-1) * UInt128(3)) was Int128
# (a negative-signed × UInt128 pair reached the legacy VM path and kept the
# signed tag). All expected values below match upstream julia 1.12.

using Test

@testset "Signed×Unsigned additive/multiplicative (Issue #9441)" begin
    @testset "128-bit same width: unsigned wins" begin
        @test typeof(Int128(-1) * UInt128(3)) === UInt128
        @test typeof(Int128(-1) + UInt128(3)) === UInt128
        @test typeof(Int128(-1) - UInt128(3)) === UInt128
        @test typeof(UInt128(3) * Int128(-1)) === UInt128
        @test typeof(UInt128(3) + Int128(-1)) === UInt128
        @test typeof(UInt128(3) - Int128(-1)) === UInt128

        # Values match `x % UInt128` sign-extend + wrapping arithmetic.
        @test Int128(-1) + UInt128(3) === UInt128(2)
        @test Int128(5) * UInt128(3) === UInt128(15)
        @test UInt128(3) - Int128(-1) === UInt128(4)
        @test Int128(-1) * UInt128(3) === typemax(UInt128) - UInt128(2)
    end

    @testset "narrower signed × UInt128 promotes to UInt128" begin
        @test typeof(UInt128(3) + Int64(-1)) === UInt128
        @test typeof(UInt128(3) + Int32(-7)) === UInt128
        @test typeof(UInt128(3) + Int8(-3)) === UInt128
        @test typeof(Int64(-1) * UInt128(3)) === UInt128

        @test UInt128(3) + Int64(-1) === UInt128(2)
        @test UInt128(100) - Int8(-3) === UInt128(103)
        @test UInt128(0) + Int32(-7) === typemax(UInt128) - UInt128(6)
    end

    @testset "narrow same-width pairs stay unsigned (regression guard)" begin
        @test typeof(Int8(-1) + UInt8(3)) === UInt8
        @test typeof(Int16(-1) * UInt16(3)) === UInt16
        @test typeof(Int32(-1) - UInt32(3)) === UInt32
        @test typeof(Int64(-1) * UInt64(3)) === UInt64
        @test Int8(-1) + UInt8(3) === UInt8(2)
        @test Int64(-1) * UInt64(3) === typemax(UInt64) - UInt64(2)
    end
end

true
