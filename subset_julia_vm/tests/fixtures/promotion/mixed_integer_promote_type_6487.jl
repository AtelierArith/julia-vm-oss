using Test

@testset "mixed integer promote_type uses concrete upstream result types" begin
    @test promote_type(Int16, Int8) === Int16
    @test promote_type(Int8, Int16) === Int16
    @test promote_type(UInt16, Int8) === UInt16
    @test promote_type(Int8, UInt16) === UInt16
    @test promote_type(UInt8, Int8) === UInt8
    @test promote_type(Int8, UInt8) === UInt8
    @test promote_type(Int16, UInt8) === Int16
    @test promote_type(UInt8, Int16) === Int16
    @test promote_type(Int128, UInt64) === Int128
    @test promote_type(UInt64, Int128) === Int128
    @test promote_type(BigInt, Int8) === BigInt
    @test promote_type(Int8, BigInt) === BigInt
end

@testset "mixed integer promote converts values to the concrete result type" begin
    p = promote(Int16(10), Int8(3))
    @test typeof(p[1]) === Int16
    @test typeof(p[2]) === Int16
    @test p == (Int16(10), Int16(3))

    p = promote(UInt16(10), Int8(3))
    @test typeof(p[1]) === UInt16
    @test typeof(p[2]) === UInt16
    @test p == (UInt16(10), UInt16(3))

    p = promote(UInt8(10), Int16(3))
    @test typeof(p[1]) === Int16
    @test typeof(p[2]) === Int16
    @test p == (Int16(10), Int16(3))

    # Value-level BigInt/narrow integer conversion remains tracked separately
    # by Issue #6489; this fixture keeps #6487 focused on promote_type plus
    # primitive signed/unsigned value conversion.
end

true
