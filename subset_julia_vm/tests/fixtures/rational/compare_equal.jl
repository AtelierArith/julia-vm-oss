# Test equality comparison for Rationals
# Note: Without normalization, 2//4 != 1//2 (different num/den)

using Test

@testset "Equality comparison for Rationals" begin
    r1 = 1 // 2
    r2 = 1 // 2
    result = 0.0
    if r1 == r2
        result = 1.0
    end
    @test (result) == 1.0
end

true  # Test passed
