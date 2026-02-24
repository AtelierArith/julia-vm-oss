# Test Rational{Int64} - Int arithmetic

using Test

@testset "Rational - Int: 7//4 - 1" begin
    r = 7 // 4  # 1.75
    result = r - 1  # 1.75 - 1 = 0.75 = 3/4
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.75)
end

true  # Test passed
