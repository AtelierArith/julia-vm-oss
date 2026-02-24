# Test signbit for Rational type (Issue #491)
# Based on Julia's base/rational.jl:365

using Test

@testset "signbit predicate for Rational (Issue #491)" begin

    result = 0.0

    # Positive rationals have signbit false
    if !signbit(Rational(1, 2))
        result = result + 1.0
    end

    # Negative rationals have signbit true
    if signbit(Rational(-1, 2))
        result = result + 1.0
    end

    # Zero has signbit false
    if !signbit(Rational(0, 1))
        result = result + 1.0
    end

    # Rational with negative denominator (normalized)
    # Note: Rational constructor normalizes 1//-2 to -1//2
    if signbit(Rational(1, -2))  # normalized to -1//2
        result = result + 1.0
    end

    if !signbit(Rational(-1, -2))  # normalized to 1//2
        result = result + 1.0
    end

    @test (result) == 5.0
end

true  # Test passed
