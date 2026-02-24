# Test floor function for Rational

using Test

@testset "floor function for Rational 7//3" begin
    r = 7 // 3  # 2.333...
    @test (floor(r)) == 2.0
end

true  # Test passed
