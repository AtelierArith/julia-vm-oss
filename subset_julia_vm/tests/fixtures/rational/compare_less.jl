# Test less-than comparison for Rationals

using Test

@testset "Less-than comparison: 1//3 < 1//2" begin
    r1 = 1 // 3
    r2 = 1 // 2
    result = 0.0
    if r1 < r2
        result = 1.0
    end
    @test (result) == 1.0
end

true  # Test passed
