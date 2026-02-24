# Test symmetry of commutative operators for Int and Rational (Issue #1785)
# Verifies n op r == r op n for commutative operators + and *

using Test

@testset "Commutative symmetry: Int + Rational == Rational + Int" begin
    r = 3 // 4
    n = 5
    # n + r
    result1 = n + r
    val1 = Float64(result1.num) / Float64(result1.den)
    # r + n
    result2 = r + n
    val2 = Float64(result2.num) / Float64(result2.den)
    @test isapprox(val1, val2)
    @test isapprox(val1, 5.75)
end

@testset "Commutative symmetry: Int * Rational == Rational * Int" begin
    r = 3 // 4
    n = 5
    # n * r
    result1 = n * r
    val1 = Float64(result1.num) / Float64(result1.den)
    # r * n
    result2 = r * n
    val2 = Float64(result2.num) / Float64(result2.den)
    @test isapprox(val1, val2)
    @test isapprox(val1, 3.75)
end

true  # Test passed
