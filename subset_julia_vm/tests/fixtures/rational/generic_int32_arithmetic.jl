using Test

@testset "Rational{Int32} arithmetic" begin
    a = Int32(1) // Int32(2)
    b = Int32(1) // Int32(3)

    # Addition: 1//2 + 1//3 = 5//6
    r_add = a + b
    @test typeof(r_add) == Rational{Int32}
    @test numerator(r_add) == 5
    @test denominator(r_add) == 6

    # Subtraction: 1//2 - 1//3 = 1//6
    r_sub = a - b
    @test typeof(r_sub) == Rational{Int32}
    @test numerator(r_sub) == 1
    @test denominator(r_sub) == 6

    # Multiplication: 1//2 * 1//3 = 1//6
    r_mul = a * b
    @test typeof(r_mul) == Rational{Int32}
    @test numerator(r_mul) == 1
    @test denominator(r_mul) == 6

    # Division: (1//2) / (1//3) = 3//2
    r_div = a / b
    @test typeof(r_div) == Rational{Int32}
    @test numerator(r_div) == 3
    @test denominator(r_div) == 2

    # Negation
    r_neg = -a
    @test typeof(r_neg) == Rational{Int32}
    @test numerator(r_neg) == -1
    @test denominator(r_neg) == 2
end

true
