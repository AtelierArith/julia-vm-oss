# Test round function for Rational

using Test

@testset "round function for Rational 5//3" begin
    r = 5 // 3  # 1.666...
    @test (round(r)) == 2.0
end

true  # Test passed
