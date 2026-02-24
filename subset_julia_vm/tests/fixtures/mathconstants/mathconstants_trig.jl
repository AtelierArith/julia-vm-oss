using Test

# Trigonometric identities using mathematical constants
@testset "trig identities with pi and e" begin
    # sin(pi) ≈ 0
    @test abs(sin(π)) < 1e-14

    # cos(2*pi) ≈ 1
    @test abs(cos(2π) - 1.0) < 1e-14

    # tan(pi/4) ≈ 1
    @test abs(tan(π/4) - 1.0) < 1e-14

    # sin²(x) + cos²(x) = 1 (Pythagorean identity)
    x = π / 3
    @test abs(sin(x)^2 + cos(x)^2 - 1.0) < 1e-14

    # exp(i*pi) = -1 (Euler's formula via real parts)
    # Using: cos(pi) = -1
    @test abs(cos(π) + 1.0) < 1e-14

    # log base e: log(e^2) = 2
    @test abs(log(exp(2.0)) - 2.0) < 1e-14
end

true
