# Test Unicode math operators
# √x => sqrt(x), ∛x => cbrt(x), ∜x => fourthroot(x)
# a ≈ b => isapprox(a, b), a ≉ b => !isapprox(a, b)

using Test

@testset "Unicode math operators: √ ∛ ∜ ≈ ≉" begin

    # Square root operator
    result1 = √16 == 4.0
    result2 = √2 ≈ 1.4142135623730951

    # Cube root operator
    result3 = ∛27 == 3.0
    result4 = ∛8 == 2.0

    # Fourth root operator
    result5 = ∜16 == 2.0
    result6 = ∜81 == 3.0

    # Approximate equality
    result7 = 1.0 ≈ 1.0000000001
    result8 = 0.1 + 0.2 ≈ 0.3
    result9 = !(1.0 ≈ 2.0)

    # Not approximately equal
    result10 = 1.0 ≉ 2.0
    result11 = !(1.0 ≉ 1.0000000001)

    # All tests pass: 11 true values summed = 11.0
    @test (Float64(result1) + Float64(result2) + Float64(result3) + Float64(result4) + Float64(result5) + Float64(result6) + Float64(result7) + Float64(result8) + Float64(result9) + Float64(result10) + Float64(result11)) == 11.0
end

true  # Test passed
