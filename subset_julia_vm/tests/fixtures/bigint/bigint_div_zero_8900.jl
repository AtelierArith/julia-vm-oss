using Test

# Issue #8900: BigInt `/` should return BigFloat Inf/NaN on division by zero,
# matching upstream Julia, instead of exiting with a DivisionByZero error.
# BigInt `÷` (integer div) still returns BigInt for non-zero denominator.
# Also verifies that div(::BigInt, ::BigInt) returns BigInt (not BigFloat).

let
    # `/` on BigInt returns BigFloat
    @test typeof(big(10) / big(3)) == BigFloat
    @test typeof(big(10) / big(0)) == BigFloat  # Inf, not an error
    @test isinf(big(10) / big(0))
    @test big(10) / big(0) > 0    # +Inf
    @test isinf(-big(10) / big(0))
    @test -big(10) / big(0) < 0   # -Inf
    @test isnan(big(0) / big(0))

    # `div` on BigInt returns BigInt (integer division)
    @test typeof(div(big(10), big(3))) == BigInt
    @test div(big(10), big(3)) == 3

    # `÷` on BigInt also returns BigInt
    @test typeof(big(10) ÷ big(3)) == BigInt
    @test big(10) ÷ big(3) == 3
end

true
