# Test power operator with negative exponent

using Test

@testset "Power with negative exponent: (2//3)^-2" begin
    r = 2 // 3
    result = r ^ -2  # (2/3)^-2 = (3/2)^2 = 9/4
    @test isapprox((Float64(result.num) / Float64(result.den)), 2.25)
end

true  # Test passed
