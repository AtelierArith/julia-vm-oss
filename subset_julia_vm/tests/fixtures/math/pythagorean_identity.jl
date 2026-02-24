# Property test: sin(θ)² + cos(θ)² ≈ 1 for θ ∈ [-π, π]
# Reference: Pythagorean trigonometric identity

using Test

@testset "sin²(θ) + cos²(θ) == 1 (property test)" begin
    for theta in -pi:0.01:pi
        @test sin(theta)^2 + cos(theta)^2 ≈ 1.0 atol=1e-15
    end
end

true
