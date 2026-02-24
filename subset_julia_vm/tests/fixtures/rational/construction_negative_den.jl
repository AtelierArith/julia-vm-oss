# Test Rational construction with negative denominator
# 3//-4 normalizes to -3//4 (sign moved to numerator)

using Test

@testset "Rational with negative denominator normalized (-3//4)" begin
    r = 3 // -4
    @test (Float64(r.num) + Float64(r.den)) == 1.0
end

true  # Test passed
