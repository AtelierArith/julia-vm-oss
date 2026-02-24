# Test float conversion for Rational

using Test

@testset "float conversion for Rational" begin
    r = 3 // 4
    @test isapprox((float(r)), 0.75)
end

true  # Test passed
