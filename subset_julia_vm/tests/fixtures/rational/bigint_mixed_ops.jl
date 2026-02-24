using Test

# Issue #2497: BigInt + Rational mixed-type operations
@testset "BigInt + Rational mixed operations" begin
    x = big(2)
    y = 1//3

    # Addition
    r1 = x + y
    @test numerator(r1) == big(7)
    @test denominator(r1) == big(3)
    @test typeof(r1) == Rational{BigInt}

    # Subtraction
    r2 = x - y
    @test numerator(r2) == big(5)
    @test denominator(r2) == big(3)
    @test typeof(r2) == Rational{BigInt}

    # Multiplication
    r3 = x * y
    @test numerator(r3) == big(2)
    @test denominator(r3) == big(3)
    @test typeof(r3) == Rational{BigInt}

    # Division
    r4 = x / y
    @test numerator(r4) == big(6)
    @test denominator(r4) == big(1)
    @test typeof(r4) == Rational{BigInt}

    # Reverse order
    r5 = y + x
    @test numerator(r5) == big(7)
    @test denominator(r5) == big(3)
    @test typeof(r5) == Rational{BigInt}

    r6 = y - x
    @test numerator(r6) == big(-5)
    @test denominator(r6) == big(3)
    @test typeof(r6) == Rational{BigInt}

    r7 = y * x
    @test numerator(r7) == big(2)
    @test denominator(r7) == big(3)
    @test typeof(r7) == Rational{BigInt}

    r8 = y / x
    @test numerator(r8) == big(1)
    @test denominator(r8) == big(6)
    @test typeof(r8) == Rational{BigInt}
end

true
