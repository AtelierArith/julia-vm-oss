# Test @evalpoly macro: Polynomial evaluation using Horner's method
# @evalpoly(z, c0, c1, c2, ...) = c0 + c1*z + c2*z^2 + ...

using Test

@testset "@evalpoly macro: Polynomial evaluation using Horner's method" begin

    # === Basic polynomial evaluation ===
    # 1 + 2x + 3x^2 at x=2: 1 + 4 + 12 = 17
    r1 = @evalpoly(2, 1, 2, 3) == 17

    # 1 + 0x + 1x^2 at x=3: 1 + 0 + 9 = 10
    r2 = @evalpoly(3, 1, 0, 1) == 10

    # Constant polynomial: just 5
    r3 = @evalpoly(100, 5) == 5

    # Linear polynomial: 1 + 2x at x=3: 1 + 6 = 7
    r4 = @evalpoly(3, 1, 2) == 7

    # === Float coefficients ===
    # 1.0 + 2.0x at x=1.5: 1.0 + 3.0 = 4.0
    r5 = abs(@evalpoly(1.5, 1.0, 2.0) - 4.0) < 1e-10

    # 0.5 + 0.25x + 0.125x^2 at x=2.0: 0.5 + 0.5 + 0.5 = 1.5
    r6 = abs(@evalpoly(2.0, 0.5, 0.25, 0.125) - 1.5) < 1e-10

    # === Using evalpoly function directly ===
    r7 = evalpoly(2, (1, 2, 3)) == 17
    r8 = evalpoly(3, (1, 0, 1)) == 10

    # Sum all results: 8 tests
    @test (Float64(r1) + Float64(r2) + Float64(r3) + Float64(r4) + Float64(r5) + Float64(r6) + Float64(r7) + Float64(r8)) == 8.0
end

true  # Test passed
