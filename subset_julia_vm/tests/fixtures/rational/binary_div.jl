# Test division of two Rationals

using Test

@testset "Division of two Rationals: 1//2 / 3//4 = 2//3" begin
    r1 = 1 // 2
    r2 = 3 // 4
    result = r1 / r2  # (1/2) / (3/4) = 4/6 = 2/3
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.6666666666666666)
end

true  # Test passed
