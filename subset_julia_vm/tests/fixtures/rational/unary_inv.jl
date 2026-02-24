# Test inv (reciprocal) for Rational

using Test

@testset "inv (reciprocal) for Rational" begin
    r = 3 // 4
    inv_r = inv(r)
    @test isapprox((Float64(inv_r.num) / Float64(inv_r.den)), 1.3333333333333333)
end

true  # Test passed
