# Rational arithmetic results should be simplified by GCD (Issue #2091)
# In Julia, Rational arithmetic always produces canonical (lowest terms) results.

using Test

@testset "Rational arithmetic GCD reduction (Issue #2091)" begin
    # Addition: 1//3 + 1//3 = 2//3 (not 6//9)
    r = 1//3 + 1//3
    @test r == 2//3
    @test r.num == 2
    @test r.den == 3

    # Addition: 1//2 + 1//4 = 3//4 (not 6//8)
    r2 = 1//2 + 1//4
    @test r2 == 3//4
    @test r2.num == 3
    @test r2.den == 4

    # Subtraction: 5//6 - 1//3 = 1//2 (not 3//6)
    r3 = 5//6 - 1//3
    @test r3 == 1//2
    @test r3.num == 1
    @test r3.den == 2

    # Multiplication: 2//3 * 3//4 = 1//2 (not 6//12)
    r4 = 2//3 * 3//4
    @test r4 == 1//2
    @test r4.num == 1
    @test r4.den == 2

    # Division: (2//3) / (4//3) = 1//2 (not 6//12)
    r5 = (2//3) / (4//3)
    @test r5 == 1//2
    @test r5.num == 1
    @test r5.den == 2

    # Mixed Rational-Int: 1//6 + 1 = 7//6
    r6 = 1//6 + 1
    @test r6 == 7//6

    # Mixed Rational-Int: 3//6 * 2 = 1//1 (simplified)
    r7 = 3//6 * 2
    @test r7 == 1//1
    @test r7.num == 1
    @test r7.den == 1
end

true
