# Test mod2pi() and rem2pi() functions (Issue #1877)

using Test

@testset "mod2pi basic" begin
    # mod2pi returns value in [0, 2π)
    @test abs(mod2pi(0.0)) < 1e-14
    @test abs(mod2pi(pi) - pi) < 1e-14
    @test abs(mod2pi(2.0 * pi)) < 1e-14
end

@testset "mod2pi negative" begin
    r = mod2pi(-pi)
    @test abs(r - pi) < 1e-14
end

@testset "mod2pi large" begin
    r = mod2pi(3.0 * pi)
    @test abs(r - pi) < 1e-13
end

@testset "rem2pi basic" begin
    # rem2pi returns value in [-π, π]
    @test abs(rem2pi(0.0)) < 1e-14
    @test abs(rem2pi(pi) - pi) < 1e-14
end

@testset "rem2pi wrap" begin
    # 3π should wrap to π (mod 2π = π, which is <= π, stays)
    r = rem2pi(3.0 * pi)
    @test abs(r - pi) < 1e-13
end

@testset "rem2pi negative wrap" begin
    # -π is at the boundary, result should be close to -π or π
    r = rem2pi(-pi)
    # mod(-π, 2π) = π, which is <= π, so stays at π
    @test abs(r - pi) < 1e-14
end

true
