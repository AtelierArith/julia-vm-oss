using Test

@testset "mixed-width integer div preserves upstream result types" begin
    @test typeof(div(Int16(10), Int8(3))) === Int16
    @test div(Int16(10), Int8(3)) == Int16(3)
    @test typeof(div(Int8(10), Int16(3))) === Int16
    @test div(Int8(10), Int16(3)) == Int16(3)
    @test typeof(div(Int64(10), Int8(3))) === Int64
    @test typeof(div(Int128(10), Int64(3))) === Int128

    @test typeof(div(UInt16(10), UInt8(3))) === UInt16
    @test div(UInt16(10), UInt8(3)) == UInt16(3)
    @test typeof(div(UInt8(10), UInt16(3))) === UInt16
    @test div(UInt8(10), UInt16(3)) == UInt16(3)
end

@testset "mixed signed unsigned integer div follows upstream direction" begin
    @test typeof(div(Int16(10), UInt8(3))) === Int16
    @test div(Int16(10), UInt8(3)) == Int16(3)
    @test typeof(div(UInt8(10), Int16(3))) === UInt16
    @test div(UInt8(10), Int16(3)) == UInt16(3)
    @test typeof(div(Int8(10), UInt16(3))) === Int16
    @test div(Int8(10), UInt16(3)) == Int16(3)
    @test typeof(div(UInt16(10), Int8(3))) === UInt16
    @test div(UInt16(10), Int8(3)) == UInt16(3)
end

@testset "mixed integer div operator and BigInt pairs" begin
    @test typeof(Int16(10) ÷ Int8(3)) === Int16
    @test (Int16(10) ÷ Int8(3)) == Int16(3)

    @test typeof(div(big(10), Int8(3))) === BigInt
    @test div(big(10), Int8(3)) == big(3)
    @test typeof(div(UInt8(10), big(3))) === BigInt
    @test div(UInt8(10), big(3)) == big(3)
    @test typeof(div(big(10), UInt8(3))) === BigInt
    @test div(big(10), UInt8(3)) == big(3)
end

true
