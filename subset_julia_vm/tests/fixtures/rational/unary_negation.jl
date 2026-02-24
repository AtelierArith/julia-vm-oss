# Test unary negation for Rational

using Test

@testset "Unary negation for Rational" begin
    r = 3 // 4
    neg_r = -r
    @test isapprox((Float64(neg_r.num) / Float64(neg_r.den)), -0.75)
end

true  # Test passed
