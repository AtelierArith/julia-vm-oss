# Test Int - Rational{Int64} arithmetic (Issue #1783)

using Test

@testset "Int - Rational{Int64} - integer minus rational number" begin
    r = 3 // 2  # 3/2 = 1.5
    result = 5 - r  # Should be 7//2 = 3.5
    # Convert to float for comparison
    @test isapprox((Float64(result.num) / Float64(result.den)), 3.5)
end

true  # Test passed
