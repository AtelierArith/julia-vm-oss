# Test power operator with zero exponent

using Test

@testset "Power with zero exponent: (2//3)^0" begin
    r = 2 // 3
    result = r ^ 0  # Any non-zero number ^ 0 = 1
    @test (Float64(result.num) / Float64(result.den)) == 1.0
end

true  # Test passed
