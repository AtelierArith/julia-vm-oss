using Test
using Base.MathConstants: γ, eulergamma, φ, golden

# Precision and relationship tests for mathematical constants
@testset "mathematical constants precision and relationships" begin
    # Golden ratio identity: φ = (1 + √5) / 2
    @test abs(φ - (1 + sqrt(5)) / 2) < 1e-14

    # Golden ratio property: φ * (φ - 1) = 1
    @test abs(φ * (φ - 1) - 1.0) < 1e-14

    # Euler–Mascheroni constant is between 0.5 and 0.6
    @test γ > 0.5
    @test γ < 0.6

    # γ == eulergamma alias
    @test γ == eulergamma

    # φ == golden alias
    @test φ == golden

    # π > 3 and π < 4
    @test π > 3.0
    @test π < 4.0

    # Reciprocal of π
    @test abs(1/π - 0.3183098861837907) < 1e-14
end

true
