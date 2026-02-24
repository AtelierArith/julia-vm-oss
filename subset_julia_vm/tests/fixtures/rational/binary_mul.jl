# Test multiplication of two Rationals

using Test

@testset "Multiplication of two Rationals: 2//3 * 3//5 = 2//5" begin
    r1 = 2 // 3
    r2 = 3 // 5
    result = r1 * r2  # 6/15 = 2/5
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.4)
end

true  # Test passed
