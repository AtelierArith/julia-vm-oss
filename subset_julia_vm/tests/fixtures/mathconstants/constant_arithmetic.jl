using Test
using Base.MathConstants: φ

@testset "mathematical constant arithmetic" begin
    # sin(pi) should be approximately 0
    @test abs(sin(π)) < 1e-10
    # cos(pi) should be approximately -1
    cos_pi = cos(π)
    @test abs(cos_pi + 1.0) < 1e-10
    # exp(0) should be 1
    @test exp(0) == 1.0
    # log(e) should be 1
    @test abs(log(ℯ) - 1.0) < 1e-10
    # golden ratio: 1/phi = phi - 1
    @test abs(1/φ - (φ - 1)) < 1e-10
end

true
