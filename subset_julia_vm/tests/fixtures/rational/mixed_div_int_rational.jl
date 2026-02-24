# Test Int / Rational{Int64} arithmetic

using Test

@testset "Int / Rational: 2 / 3//4" begin
    r = 3 // 4
    result = 2 / r  # 2 / (3/4) = 8/3
    @test isapprox((Float64(result.num) / Float64(result.den)), 2.6666666666666665)
end

true  # Test passed
