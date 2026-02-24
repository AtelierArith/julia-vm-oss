# Test MathConstants module - basic constants
# MathConstants provides mathematical constants like π, ℯ, γ, φ, catalan

using Test
using Base.MathConstants: e, γ, eulergamma, φ, golden, catalan

@testset "MathConstants basic (Issue #761)" begin
    # Test π (pi)
    @test (abs(π - 3.141592653589793) < 1e-10)
    @test (abs(pi - 3.141592653589793) < 1e-10)
    @test (π == pi)

    # Test ℯ (Euler's number)
    @test (abs(ℯ - 2.718281828459045) < 1e-10)
    @test (abs(e - 2.718281828459045) < 1e-10)
    @test (ℯ == e)

    # Test γ (Euler-Mascheroni constant)
    @test (abs(γ - 0.5772156649015329) < 1e-10)
    @test (abs(eulergamma - 0.5772156649015329) < 1e-10)
    @test (γ == eulergamma)

    # Test φ (golden ratio)
    @test (abs(φ - 1.618033988749895) < 1e-10)
    @test (abs(golden - 1.618033988749895) < 1e-10)
    @test (φ == golden)

    # Test catalan (Catalan's constant)
    @test (abs(catalan - 0.9159655941772190) < 1e-10)
end

true  # Test passed
