# Test mathematical constants (Issue #484)
# Based on Julia's base/mathconstants.jl
# Note: In Julia, some constants need explicit import from MathConstants

using Test
using Base.MathConstants: e, γ, eulergamma, φ, golden, catalan

@testset "Mathematical constants: pi, e, gamma, phi, catalan (Issue #484)" begin

    # Test π (pi)
    @assert abs(π - 3.141592653589793) < 1e-15
    @assert abs(pi - 3.141592653589793) < 1e-15
    @assert π == pi

    # Test ℯ (Euler's number)
    @assert abs(ℯ - 2.718281828459045) < 1e-15

    # Test γ (Euler-Mascheroni constant)
    @assert abs(γ - 0.5772156649015329) < 1e-15
    @assert abs(eulergamma - 0.5772156649015329) < 1e-15
    @assert γ == eulergamma

    # Test φ (golden ratio)
    @assert abs(φ - 1.618033988749895) < 1e-15
    @assert abs(golden - 1.618033988749895) < 1e-15
    @assert φ == golden

    # Test catalan (Catalan's constant)
    @assert abs(catalan - 0.915965594177219) < 1e-15

    # Verify golden ratio property: φ² = φ + 1
    @assert abs(φ * φ - (φ + 1)) < 1e-14

    @test (true)
end

true  # Test passed
