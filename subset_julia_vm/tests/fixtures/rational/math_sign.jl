# Test sign function for Rational

using Test

@testset "sign function for positive, negative, and zero Rationals" begin
    r1 = 3 // 4
    r2 = -3 // 4
    r3 = 0 // 1
    @test (Float64(sign(r1) + sign(r2) * 10 + sign(r3) * 100)) == -9.0
end

true  # Test passed
