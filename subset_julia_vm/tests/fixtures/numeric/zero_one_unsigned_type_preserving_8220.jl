# Test zero(x) / one(x) preserve the unsigned integer type
# Issue #8220: `one(0x05)` / `zero(0x05)` fell through to a generic that returned
#   Int64 (or errored NoMethodFound in some static contexts) instead of the same
#   UInt type as the argument. Upstream: one(0x05) === 0x01 :: UInt8.

using Test

@testset "zero/one preserve unsigned type (Issue #8220)" begin
    @testset "one(::Unsigned)" begin
        @test one(0x05) === 0x01
        @test one(UInt16(5)) === UInt16(1)
        @test one(UInt32(5)) === UInt32(1)
        @test one(UInt64(5)) === UInt64(1)
        @test one(UInt128(5)) === UInt128(1)
        @test typeof(one(0x05)) === UInt8
        @test typeof(one(UInt64(5))) === UInt64
    end

    @testset "zero(::Unsigned)" begin
        @test zero(0x05) === 0x00
        @test zero(UInt16(5)) === UInt16(0)
        @test zero(UInt32(5)) === UInt32(0)
        @test zero(UInt64(5)) === UInt64(0)
        @test zero(UInt128(5)) === UInt128(0)
        @test typeof(zero(0x05)) === UInt8
        @test typeof(zero(UInt64(5))) === UInt64
    end

    @testset "still works through a generic function (untyped arg)" begin
        myone(x) = one(x)
        myzero(x) = zero(x)
        @test myone(0x05) === 0x01
        @test myzero(UInt16(9)) === UInt16(0)
        @test typeof(myone(0x05)) === UInt8
    end

    @testset "signed/float types unaffected" begin
        @test one(5) === 1
        @test one(Int8(5)) === Int8(1)
        @test one(2.0) === 1.0
        @test zero(Int16(3)) === Int16(0)
    end
end

true
