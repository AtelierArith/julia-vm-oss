# Mixed Signed×Unsigned comparisons and div/fld/cld/rem/mod (Issues #9336, #9337)
#
# #9336: comparisons (== != < <= > >=) on a Signed×Unsigned pair naively
#   promoted (negative -> unsigned convert) and threw InexactError instead of
#   returning the correct Bool. Upstream julia/base/int.jl uses a sign check
#   plus a same-width unsigned compare.
# #9337: div/fld/cld/rem/mod ignored upstream's per-operator signedness rules
#   (div/fld/cld/rem follow the dividend's signedness; mod follows the divisor's;
#   Bool×Bool stays Bool) and instead promoted to the unsigned-wins type, giving
#   wrong result types plus InexactError on negative dividends.

using Test

@testset "Signed×Unsigned mixed (Issues #9336 / #9337)" begin
    @testset "comparisons: negative vs unsigned return Bool, no InexactError" begin
        @test (-1 < UInt64(1)) === true
        @test (-1 == typemax(UInt64)) === false
        @test (-1 != typemax(UInt64)) === true
        @test (typemax(UInt64) > -1) === true
        @test (UInt8(255) == Int8(-1)) === false
        @test (Int8(-1) < UInt8(0)) === true
        @test (Int8(-1) <= UInt8(0)) === true
        @test (UInt8(0) > Int8(-1)) === true
        @test (UInt8(0) >= Int8(-1)) === true
        @test (typemin(Int64) < typemin(UInt64)) === true
        @test (typemax(UInt128) > typemin(Int128)) === true
        @test (Int16(5) == UInt64(5)) === true
        @test (UInt32(5) <= Int8(5)) === true
    end

    @testset "div/rem: dividend signedness, promoted width" begin
        @test rem(Int8(3), UInt8(5)) === Int8(3)
        @test typeof(rem(Int8(3), UInt64(5))) === Int64
        @test rem(Int8(-3), UInt8(5)) === Int8(-3)
        @test div(Int8(3), UInt8(5)) === Int8(0)
        @test div(Int128(-1), UInt128(5)) === Int128(0)
        @test rem(UInt8(7), Int8(-5)) === UInt8(2)
        @test div(UInt64(7), Int8(-5)) === typemax(UInt64)
    end

    @testset "mod: divisor signedness, promoted width" begin
        @test mod(Int64(3), UInt8(5)) === UInt64(3)
        @test typeof(mod(Int128(3), UInt8(5))) === UInt128
        @test mod(Int8(-3), UInt8(5)) === UInt8(2)
        @test mod(Int16(-1), UInt8(3)) === UInt16(2)
        @test mod(UInt8(7), Int8(-5)) === Int8(-3)
        @test mod(UInt64(7), Int8(-5)) === Int64(-3)
    end

    @testset "fld/cld inherit the div sign rule" begin
        @test fld(Int8(3), UInt8(5)) === Int8(0)
        @test fld(Int8(-3), UInt8(5)) === Int8(-1)
        @test cld(Int128(-1), UInt128(5)) === Int128(0)
        @test cld(Int8(3), UInt8(5)) === Int8(1)
        @test cld(UInt8(3), Int8(5)) === UInt8(1)
    end

    @testset "cld/div by zero throws DivideError (not InexactError)" begin
        @test_throws DivideError cld(Int128(-1), UInt128(0))
        @test_throws DivideError div(Int8(-3), UInt8(0))
    end

    @testset "Bool×Bool div-family stays Bool" begin
        @test mod(true, true) === false
        @test rem(true, true) === false
        @test div(true, true) === true
        @test fld(true, true) === true
        @test cld(true, true) === true
        @test_throws DivideError mod(true, false)
    end
end

true
