# Test abs function for Rational

using Test

@testset "abs function for negative Rational" begin
    r = -3 // 4
    result = abs(r)
    @test isapprox((result.num / result.den), 0.75)
end

true  # Test passed
