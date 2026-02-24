# Test Int * Rational{Int64} arithmetic

using Test

@testset "Int * Rational{Int64} - integer times rational number" begin
    r = 3 // 2  # 3/2 = 1.5
    result = 2 * r  # Should be 6//2 = 3
    # Convert to float for comparison
    @test (Float64(result.num) / Float64(result.den)) == 3.0
end

true  # Test passed
