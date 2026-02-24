# Test basic Rational construction with normalization
# 2//2 normalizes to 1//1

using Test

@testset "Basic Rational construction with normalization (2//2 -> 1//1)" begin
    r = 2 // 2
    @test (Float64(r.num) + Float64(r.den)) == 2.0
end

true  # Test passed
