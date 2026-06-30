# Test BigFloat arithmetic with the full set of primitive numeric operands.
# Verifies that `pop_bigfloat` accepts every primitive numeric Value variant,
# matching official Julia: any numeric + BigFloat -> BigFloat.
# Related: Issue #3749 (BigFloat + Float32/Float16/Int128/UInt* runtime type error)

using Test

@testset "BigFloat + Float32 / Float16 -> BigFloat" begin
    a = big(1.5)

    @test typeof(a + Float32(1.0)) == BigFloat
    @test typeof(a + Float16(1.0)) == BigFloat
    @test typeof(Float32(1.0) + a) == BigFloat
    @test typeof(Float16(1.0) + a) == BigFloat

    @test a + Float32(0.5) == big(2.0)
    @test a + Float16(0.5) == big(2.0)
    @test a - Float32(0.5) == big(1.0)
    @test a * Float32(2.0) == big(3.0)
end

@testset "BigFloat + Int128 / narrow signed -> BigFloat" begin
    a = big(1.5)

    @test typeof(a + Int128(2)) == BigFloat
    @test typeof(a + Int8(2)) == BigFloat
    @test typeof(a + Int16(2)) == BigFloat
    @test typeof(a + Int32(2)) == BigFloat
    @test typeof(Int128(2) + a) == BigFloat
    @test typeof(Int8(2) + a) == BigFloat

    @test a + Int128(2) == big(3.5)
    @test a + Int8(2) == big(3.5)
end

@testset "BigFloat + unsigned -> BigFloat" begin
    a = big(1.5)

    @test typeof(a + UInt8(2)) == BigFloat
    @test typeof(a + UInt16(2)) == BigFloat
    @test typeof(a + UInt32(2)) == BigFloat
    @test typeof(a + UInt64(2)) == BigFloat
    @test typeof(a + UInt128(2)) == BigFloat
    @test typeof(UInt8(2) + a) == BigFloat

    @test a + UInt8(2) == big(3.5)
    @test a + UInt32(2) == big(3.5)
end

@testset "BigFloat + Bool -> BigFloat" begin
    a = big(1.5)
    @test typeof(a + true) == BigFloat
    @test a + true == big(2.5)
    @test a + false == big(1.5)
end

@testset "BigFloat comparisons with all primitives" begin
    a = big(1.5)

    @test a > Int8(1)
    @test a > Int16(1)
    @test a > Int32(1)
    @test a > Int128(1)
    @test a > UInt8(1)
    @test a > UInt16(1)
    @test a > UInt32(1)
    @test a > UInt64(1)
    @test a > UInt128(1)
    @test a < Float32(2.0)
    @test a < Float16(2.0)
end

true
