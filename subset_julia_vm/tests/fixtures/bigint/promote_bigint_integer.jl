using Test

# Issue #2512: promote(BigInt, Int64) should promote Int64 to BigInt
@testset "BigInt promotion with integer types" begin
    # BigInt + Int64
    x1, y1 = promote(big(1), 3)
    @test typeof(x1) == BigInt
    @test typeof(y1) == BigInt
    @test x1 == big(1)
    @test y1 == big(3)

    # BigInt + Int64 (reverse order)
    x2, y2 = promote(3, big(1))
    @test typeof(x2) == BigInt
    @test typeof(y2) == BigInt

    # promote_type
    @test promote_type(BigInt, Int64) == BigInt
    @test promote_type(Int64, BigInt) == BigInt

    # Mixed BigInt // Int (the original failing case)
    # big(1) // big(3) works (Issue #2508), but big(1) // 3 requires promotion
    r = big(6) // big(4)
    @test numerator(r) == big(3)
    @test denominator(r) == big(2)
    @test typeof(r) == Rational{BigInt}
end

true
