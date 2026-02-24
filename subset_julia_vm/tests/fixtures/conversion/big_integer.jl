# Test big() function for integer types

using Test

@testset "big() function for integer types -> BigInt" begin

    # big(Int64) -> BigInt
    x1 = big(42)
    println(x1)  # 42

    # big(negative)
    x2 = big(-123)
    println(x2)  # -123

    # big(0)
    x3 = big(0)
    println(x3)  # 0

    # big of already BigInt (identity)
    c = BigInt(100)
    d = big(c)
    println(d)  # 100

    # Return Int64 for test assertion (BigInt can be converted to Int64)
    @test (Int64(d)) == 100
end

true  # Test passed
