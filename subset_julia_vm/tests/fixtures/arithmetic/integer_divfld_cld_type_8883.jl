using Test

# Issue #8883: div/fld/cld/rem/mod result-type drift.
# Narrow integer types should preserve their type through arithmetic ops
# in generic dispatch contexts. Bool + narrow int pairs promote to the
# narrow int type (not Int64).

let
    # rem: narrow int type preservation
    @test typeof(rem(Int8(7), Int8(3))) == Int8
    @test rem(Int8(7), Int8(3)) == Int8(1)

    @test typeof(rem(UInt8(7), UInt8(3))) == UInt8
    @test rem(UInt8(7), UInt8(3)) == UInt8(1)

    # rem(Bool, narrow_int) → narrow_int (Bool promotes to other operand's type)
    @test typeof(rem(true, UInt8(5))) == UInt8
    @test rem(true, UInt8(5)) == UInt8(1)

    @test typeof(rem(true, Int8(5))) == Int8
    @test rem(true, Int8(5)) == Int8(1)

    @test typeof(rem(UInt8(7), true)) == UInt8
    @test rem(UInt8(7), true) == UInt8(0)   # 7 rem 1 == 0

    # cld: ceiling division preserves integer type
    @test typeof(cld(Int8(5), Int8(3))) == Int8
    @test cld(Int8(5), Int8(3)) == Int8(2)

    @test typeof(cld(Int64(5), Int64(3))) == Int64
    @test cld(Int64(5), Int64(3)) == Int64(2)

    @test typeof(cld(Int8(-5), Int8(3))) == Int8
    @test cld(Int8(-5), Int8(3)) == Int8(-1)

    # mod: preserve integer type
    @test typeof(mod(Int8(7), Int8(3))) == Int8
    @test mod(Int8(7), Int8(3)) == Int8(1)

    @test typeof(mod(Int8(-1), Int8(3))) == Int8
    @test mod(Int8(-1), Int8(3)) == Int8(2)
end

true
