# Test isone predicate for Rational
# Note: Without normalization, 4//4 is not equal to 1//1, so isone returns false

using Test

@testset "isone predicate for Rational" begin
    r1 = 1 // 1  # This is exactly 1//1
    r2 = 3 // 4
    result = 0.0
    if isone(r1)
        result = result + 1.0
    end
    if isone(r2)
        result = result + 10.0
    end
    @test (result) == 1.0
end

true  # Test passed
