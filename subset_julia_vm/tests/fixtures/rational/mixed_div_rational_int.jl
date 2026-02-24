# Test Rational{Int64} / Int arithmetic

using Test

@testset "Rational / Int: 3//4 / 2" begin
    r = 3 // 4
    result = r / 2  # (3/4) / 2 = 3/8
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.375)
end

true  # Test passed
