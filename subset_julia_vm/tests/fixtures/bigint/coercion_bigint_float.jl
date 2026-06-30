# Test BigInt + Float* (Float64/Float32/Float16) promotes to BigFloat.
# Mirrors official Julia: any integer + BigFloat (or BigInt + AbstractFloat)
# produces a BigFloat.
# Related: Issue #3743 (BigInt + Float64 runtime type error)

using Test

@testset "BigInt + Float64 -> BigFloat" begin
    a = big(2)

    @test typeof(a + 1.0) == BigFloat
    @test typeof(a - 1.0) == BigFloat
    @test typeof(a * 2.0) == BigFloat
    @test typeof(a / 2.0) == BigFloat

    @test a + 1.0 == big(3.0)
    @test a - 1.0 == big(1.0)
    @test a * 2.0 == big(4.0)
    @test a / 2.0 == big(1.0)

    # Reverse: Float64 + BigInt
    @test typeof(1.0 + a) == BigFloat
    @test 1.0 + a == big(3.0)
end

@testset "BigInt + Float32 / Float16 -> BigFloat" begin
    a = big(2)

    @test typeof(a + Float32(1.0)) == BigFloat
    @test typeof(a + Float16(1.0)) == BigFloat
    @test typeof(Float32(1.0) + a) == BigFloat
    @test typeof(Float16(1.0) + a) == BigFloat

    @test a + Float32(1.0) == big(3.0)
    @test a + Float16(1.0) == big(3.0)
end

@testset "BigInt vs Float* comparisons" begin
    a = big(2)

    @test a < 3.0
    @test a == 2.0
    @test 1.0 < a
    @test a < Float32(3.0)
    @test a == Float32(2.0)
    @test a < Float16(3.0)
end

true
