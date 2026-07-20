# Test Int128 / UInt128 string macro literals (upstream Base @int128_str /
# @uint128_str: int128"..." / uint128"..."). The capitalized Int128"..." /
# UInt128"..." spellings are NOT upstream (Issues #10320 / #10324); this fixture
# uses only the upstream lowercase forms.

using Test

@testset "Int128 string macro" begin
    # Basic Int128 literal
    x = int128"123"
    @test typeof(x) == Int128
    @test x == Int128(123)

    # Large Int128 value (beyond Int64 range)
    large = int128"9223372036854775808"  # 2^63, just beyond Int64 max
    @test large > Int128(9223372036854775807)

    # Negative Int128
    neg = int128"-123"
    @test neg == Int128(-123)

    # Zero
    @test int128"0" == Int128(0)

    # Arithmetic with Int128 string macro
    a = int128"100"
    b = int128"200"
    @test a + b == Int128(300)
end

@testset "UInt128 string macro" begin
    # Basic UInt128 literal (Issue #10320)
    x = uint128"123"
    @test typeof(x) == UInt128
    @test x == UInt128(123)

    # Zero
    @test uint128"0" == UInt128(0)

    # Value above typemax(Int128): must not overflow through a signed bit pattern
    big1 = uint128"170141183460469231731687303715884105728"  # 2^127
    @test typeof(big1) == UInt128
    @test big1 == UInt128(2)^127

    # typemax(UInt128) = 2^128 - 1
    umax = uint128"340282366920938463463374607431768211455"
    @test umax == typemax(UInt128)

    # Arithmetic with UInt128 string macro
    @test uint128"100" + uint128"200" == UInt128(300)
end

true
