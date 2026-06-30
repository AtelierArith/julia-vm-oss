# Test mod2pi() upstream-compatible cases (Issues #1877, #5634)

using Test

@testset "mod2pi basic" begin
    @test abs(mod2pi(0.0)) < 1e-14
    @test abs(mod2pi(2.0 * pi) - 2.0 * pi) < 1e-14
end

@testset "mod2pi negative" begin
    r = mod2pi(-pi)
    @test abs(r - pi) < 1e-14
end

@testset "mod2pi large" begin
    r = mod2pi(3.0 * pi)
    @test abs(r - pi) < 1e-13
end

true
