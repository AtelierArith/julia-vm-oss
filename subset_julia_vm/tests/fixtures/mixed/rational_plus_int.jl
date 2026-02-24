# Test Rational{Int64} + Int arithmetic

using Test

@testset "Rational{Int64} + Int - rational number added to integer" begin
    r = 1 // 2  # 1/2 = 0.5
    result = r + 2  # Should be 5//2 = 2.5
    # Convert to float for comparison
    @test isapprox((Float64(result.num) / Float64(result.den)), 2.5)
end

true  # Test passed
