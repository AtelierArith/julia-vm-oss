# Test big() function for float types

using Test

@testset "big() function for float types -> BigFloat" begin

    # big(Float64) -> BigFloat
    x1 = big(3.14159)
    println(x1)  # 3.14159 (as BigFloat)

    # big(negative float)
    x2 = big(-2.5)
    println(x2)  # -2.5

    # big(0.0)
    x3 = big(0.0)
    println(x3)  # 0.0

    # big of already BigFloat (identity)
    c = BigFloat(100.5)
    d = big(c)
    println(d)  # 100.5

    # Verify results
    @test (d == BigFloat(100.5))
end

true  # Test passed
