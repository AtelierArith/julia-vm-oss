# Test Int128 string macro literal

using Test

@testset "Int128 string macro" begin
    # Basic Int128 literal
    x = Int128"123"
    @test typeof(x) == Int128
    @test x == Int128(123)

    # Large Int128 value (beyond Int64 range)
    large = Int128"9223372036854775808"  # 2^63, just beyond Int64 max
    @test large > Int128(9223372036854775807)

    # Negative Int128
    neg = Int128"-123"
    @test neg == Int128(-123)

    # Zero
    @test Int128"0" == Int128(0)

    # Arithmetic with Int128 string macro
    a = Int128"100"
    b = Int128"200"
    @test a + b == Int128(300)
end

true
