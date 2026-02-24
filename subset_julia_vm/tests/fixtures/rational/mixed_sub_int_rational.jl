# Test Int - Rational{Int64} arithmetic

using Test

@testset "Int - Rational: 2 - 3//4" begin
    r = 3 // 4  # 0.75
    result = 2 - r  # 2 - 0.75 = 1.25 = 5/4
    @test isapprox((Float64(result.num) / Float64(result.den)), 1.25)
end

true  # Test passed
