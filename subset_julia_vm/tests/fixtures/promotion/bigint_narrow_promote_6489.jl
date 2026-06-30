using Test

@testset "BigInt constructor accepts narrow primitive integers" begin
    @test BigInt(Int8(-3)) == big(-3)
    @test BigInt(Int16(-3)) == big(-3)
    @test BigInt(Int32(-3)) == big(-3)
    @test BigInt(UInt8(3)) == big(3)
    @test BigInt(UInt16(3)) == big(3)
    @test BigInt(UInt32(3)) == big(3)
    @test BigInt(UInt64(3)) == big(3)
    @test BigInt(UInt128(3)) == big(3)
end

@testset "promote BigInt with narrow integers converts both values" begin
    p = promote(big(10), Int8(3))
    @test typeof(p[1]) === BigInt
    @test typeof(p[2]) === BigInt
    @test p == (big(10), big(3))

    p = promote(UInt16(10), big(3))
    @test typeof(p[1]) === BigInt
    @test typeof(p[2]) === BigInt
    @test p == (big(10), big(3))

    p = promote(big(10), UInt128(3))
    @test typeof(p[1]) === BigInt
    @test typeof(p[2]) === BigInt
    @test p == (big(10), big(3))
end

true
