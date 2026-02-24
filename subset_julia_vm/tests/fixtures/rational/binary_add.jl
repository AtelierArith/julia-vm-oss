# Test addition of two Rationals

using Test

@testset "Addition of two Rationals: 1//2 + 1//3 = 5//6" begin
    r1 = 1 // 2
    r2 = 1 // 3
    result = r1 + r2  # 3/6 + 2/6 = 5/6
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.8333333333333334)
end

true  # Test passed
