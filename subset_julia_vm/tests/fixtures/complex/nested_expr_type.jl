# Regression test for Issue #2425:
# Complex arithmetic in nested expressions must preserve type.
# Previously, `cx * log(z)` returned Float64 instead of Complex{Float64}
# because the (Struct, Any) case was missing from needs_runtime_dispatch
# in the non-all-base-extensions binary operator path.

using Test

@testset "nested expression type preservation" begin
    z = Complex{Float64}(1.0, 2.0)
    cx = Complex{Float64}(2.0, 0.0)

    # Step-by-step (always worked)
    lz = log(z)
    w1 = cx * lz
    @test typeof(w1) == Complex{Float64}

    # Nested expression (was broken: returned Float64)
    w2 = cx * log(z)
    @test typeof(w2) == Complex{Float64}

    # Values must match
    @test real(w1) == real(w2)
    @test imag(w1) == imag(w2)

    # Test with exp (also affected)
    e1 = exp(z)
    w3 = cx * e1
    w4 = cx * exp(z)
    @test typeof(w4) == Complex{Float64}
    @test real(w3) == real(w4)
    @test imag(w3) == imag(w4)

    # Test + and - with nested calls (were not affected but verify)
    w5 = cx + log(z)
    @test typeof(w5) == Complex{Float64}

    w6 = cx - log(z)
    @test typeof(w6) == Complex{Float64}
end

true
