# Test Rational construction with reduction
# 6//9 normalizes to 2//3 (divided by gcd=3)

using Test

@testset "Rational construction with reduction (6//9 -> 2//3)" begin
    r = 6 // 9
    @test (Float64(r.num) + Float64(r.den)) == 5.0
end

true  # Test passed
