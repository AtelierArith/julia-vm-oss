using Test

@testset "Rational{Int16} basic" begin
    # Construction via //
    r = Int16(3) // Int16(2)
    @test typeof(r) == Rational{Int16}
    @test numerator(r) == 3
    @test denominator(r) == 2

    # GCD reduction
    r2 = Int16(6) // Int16(4)
    @test numerator(r2) == 3
    @test denominator(r2) == 2

    # Direct constructor
    r3 = Rational(Int16(5), Int16(1))
    @test typeof(r3) == Rational{Int16}
    @test numerator(r3) == 5
    @test denominator(r3) == 1
end

true
