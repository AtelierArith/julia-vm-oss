using Test

@testset "Rational{Int32} basic construction" begin
    # Direct constructor
    r = Rational(Int32(3), Int32(2))
    @test typeof(r) == Rational{Int32}
    @test numerator(r) == 3
    @test denominator(r) == 2

    # // operator
    r2 = Int32(3) // Int32(2)
    @test typeof(r2) == Rational{Int32}
    @test numerator(r2) == 3
    @test denominator(r2) == 2

    # GCD reduction
    r3 = Rational(Int32(6), Int32(4))
    @test numerator(r3) == 3
    @test denominator(r3) == 2

    # Negative denominator normalization
    r4 = Rational(Int32(3), Int32(-2))
    @test numerator(r4) == -3
    @test denominator(r4) == 2

    # Single argument constructor
    r5 = Rational(Int32(5))
    @test typeof(r5) == Rational{Int32}
    @test numerator(r5) == 5
    @test denominator(r5) == 1
end

true
