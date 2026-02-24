# Test Int + Rational{Int64} arithmetic

using Test

@testset "Int + Rational{Int64} - integer added to rational number" begin
    r = 3 // 2  # 3/2 = 1.5
    result = 1 + r  # Should be 5//2 = 2.5
    # Convert to float for comparison
    @test isapprox((Float64(result.num) / Float64(result.den)), 2.5)
end

true  # Test passed
