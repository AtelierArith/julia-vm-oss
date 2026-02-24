# Test inverse reciprocal hyperbolic functions
# - asech: inverse hyperbolic secant, acosh(1/x)
# - acsch: inverse hyperbolic cosecant, asinh(1/x)
# - acoth: inverse hyperbolic cotangent, atanh(1/x)

using Test

@testset "Inverse reciprocal hyperbolic functions: asech, acsch, acoth" begin

    # === asech: inverse hyperbolic secant ===
    # asech(x) = acosh(1/x)
    # Domain: x > 0 and x <= 1
    r1 = abs(asech(1.0) - 0.0) < 1e-10   # asech(1) = acosh(1) = 0
    r2 = abs(asech(0.5) - acosh(2.0)) < 1e-10   # asech(0.5) = acosh(2)
    r3 = abs(asech(0.1) - acosh(10.0)) < 1e-10   # asech(0.1) = acosh(10)

    # === acsch: inverse hyperbolic cosecant ===
    # acsch(x) = asinh(1/x)
    # Domain: x != 0
    r4 = abs(acsch(1.0) - asinh(1.0)) < 1e-10   # acsch(1) = asinh(1)
    r5 = abs(acsch(2.0) - asinh(0.5)) < 1e-10   # acsch(2) = asinh(0.5)
    r6 = abs(acsch(-1.0) - asinh(-1.0)) < 1e-10   # acsch(-1) = asinh(-1)

    # === acoth: inverse hyperbolic cotangent ===
    # acoth(x) = atanh(1/x)
    # Domain: |x| > 1
    r7 = abs(acoth(2.0) - atanh(0.5)) < 1e-10   # acoth(2) = atanh(0.5)
    r8 = abs(acoth(-2.0) - atanh(-0.5)) < 1e-10   # acoth(-2) = atanh(-0.5)
    r9 = abs(acoth(10.0) - atanh(0.1)) < 1e-10   # acoth(10) = atanh(0.1)

    # Sum all results: 9 tests
    @test (Float64(r1) + Float64(r2) + Float64(r3) + Float64(r4) + Float64(r5) + Float64(r6) + Float64(r7) + Float64(r8) + Float64(r9)) == 9.0
end

true  # Test passed
