# Test power operator with positive exponent

using Test

@testset "Power with positive exponent: (2//3)^3" begin
    r = 2 // 3
    result = r ^ 3  # (2/3)^3 = 8/27
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.2962962962962963)
end

true  # Test passed
