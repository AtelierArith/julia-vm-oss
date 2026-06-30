using Test

@testset "Rational // Rational dispatch (Issue #8255)" begin
    q = (big(3)//big(4)) // (big(3)//big(2))
    @test q == big(1)//big(2)
    @test typeof(q) === Rational{BigInt}
    @test numerator(q) == big(1)
    @test denominator(q) == big(2)

    r = (3//5) // (2//1)
    @test r == 3//10
    @test typeof(r) === Rational{Int64}
end

true
