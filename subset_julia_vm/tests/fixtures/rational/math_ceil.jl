# Test ceil function for Rational

using Test

@testset "ceil function for Rational 7//3" begin
    r = 7 // 3  # 2.333...
    @test (ceil(r)) == 3.0
end

true  # Test passed
