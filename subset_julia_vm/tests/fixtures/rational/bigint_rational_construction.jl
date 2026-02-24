using Test

# Issue #2508: BigInt Rational construction via // operator
@testset "BigInt Rational construction" begin
    # Basic construction
    r1 = big(1) // big(3)
    @test numerator(r1) == big(1)
    @test denominator(r1) == big(3)
    @test typeof(r1) == Rational{BigInt}

    # GCD reduction
    r2 = big(2) // big(4)
    @test numerator(r2) == big(1)
    @test denominator(r2) == big(2)
    @test typeof(r2) == Rational{BigInt}

    # Negative numerator
    r3 = big(-3) // big(4)
    @test numerator(r3) == big(-3)
    @test denominator(r3) == big(4)
    @test typeof(r3) == Rational{BigInt}

    # Negative denominator (normalized to positive)
    r4 = big(3) // big(-4)
    @test numerator(r4) == big(-3)
    @test denominator(r4) == big(4)
    @test typeof(r4) == Rational{BigInt}

    # Large numbers
    r5 = big(1000000000000) // big(3)
    @test numerator(r5) == big(1000000000000)
    @test denominator(r5) == big(3)
    @test typeof(r5) == Rational{BigInt}

    # Zero numerator
    r6 = big(0) // big(5)
    @test numerator(r6) == big(0)
    @test denominator(r6) == big(1)
    @test typeof(r6) == Rational{BigInt}

    # Unit fraction
    r7 = big(1) // big(1)
    @test numerator(r7) == big(1)
    @test denominator(r7) == big(1)
    @test typeof(r7) == Rational{BigInt}
end

true
