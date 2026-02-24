# Test rationalize() function - convert float to rational approximation
# rationalize(x::Float64) -> Rational{Int64}
# rationalize(x::Rational) -> Rational (identity)
# rationalize(x::Int64) -> Rational{Int64}

using Test

@testset "rationalize() - convert float to rational approximation" begin

    result = 0.0

    # Test basic float to rational conversion
    r1 = rationalize(5.6)
    if numerator(r1) == 28 && denominator(r1) == 5
        result = result + 1.0
    end

    # Test simple fraction (0.5 = 1/2)
    r2 = rationalize(0.5)
    if numerator(r2) == 1 && denominator(r2) == 2
        result = result + 1.0
    end

    # Test integer (3.0 = 3/1)
    r3 = rationalize(3.0)
    if numerator(r3) == 3 && denominator(r3) == 1
        result = result + 1.0
    end

    # Test rationalize on already rational (identity)
    r4 = rationalize(3//2)
    if numerator(r4) == 3 && denominator(r4) == 2
        result = result + 1.0
    end

    # Test rationalize on integer
    r5 = rationalize(42)
    if numerator(r5) == 42 && denominator(r5) == 1
        result = result + 1.0
    end

    # Test with tolerance
    r6 = rationalize(10.3)
    # Should approximate 10.3 = 103/10
    if numerator(r6) == 103 && denominator(r6) == 10
        result = result + 1.0
    end

    @test (result) == 6.0
end

true  # Test passed
