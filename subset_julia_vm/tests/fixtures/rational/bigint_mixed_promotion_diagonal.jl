using Test

@testset "BigInt + Rational promotion diagonal dispatch" begin
    x = big(2)
    y = 1//3

    promoted = promote(x, y)
    @test typeof(promoted) == Tuple{Rational{BigInt}, Rational{BigInt}}
    @test typeof(promoted[1]) == Rational{BigInt}
    @test typeof(promoted[2]) == Rational{BigInt}
    @test numerator(promoted[1]) == big(2)
    @test denominator(promoted[1]) == big(1)
    @test numerator(promoted[2]) == big(1)
    @test denominator(promoted[2]) == big(3)

    reverse_promoted = promote(y, x)
    @test typeof(reverse_promoted[1]) == Rational{BigInt}
    @test typeof(reverse_promoted[2]) == Rational{BigInt}

    r = x + y
    @test typeof(r) == Rational{BigInt}
    @test numerator(r) == big(7)
    @test denominator(r) == big(3)

    @test typeof(y + x) == Rational{BigInt}
    @test typeof(x - y) == Rational{BigInt}
    @test typeof(y - x) == Rational{BigInt}
    @test typeof(x * y) == Rational{BigInt}
    @test typeof(y * x) == Rational{BigInt}
    @test typeof(x / y) == Rational{BigInt}
    @test typeof(y / x) == Rational{BigInt}
end

true
