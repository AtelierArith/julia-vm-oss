# Test subtraction of two Rationals

using Test

@testset "Subtraction of two Rationals: 3//4 - 1//4 = 1//2" begin
    r1 = 3 // 4
    r2 = 1 // 4
    result = r1 - r2  # 2/4 = 1/2
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.5)
end

true  # Test passed
