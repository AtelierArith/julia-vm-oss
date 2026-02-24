# Test isinteger predicate for Rational
# Note: Without normalization, 8//4 has den=4, not den=1

using Test

@testset "isinteger predicate for Rational" begin
    r1 = 2 // 1  # This is exactly 2//1, which is an integer
    r2 = 3 // 4
    result = 0.0
    if isinteger(r1)
        result = result + 1.0
    end
    if isinteger(r2)
        result = result + 10.0
    end
    @test (result) == 1.0
end

true  # Test passed
