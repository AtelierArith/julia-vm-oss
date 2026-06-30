using Test

# Issue #5768: Complex{<:Integer}^Integer (e.g. (2+3im)^2, im^2) previously hung
# (no method; only Complex{Float64} powers existed). Mirrors upstream
# `^(z::Complex{<:Integer}, n::Integer) = power_by_squaring(z, n)`, with
# to_power_type promotion so `im` (a Complex{Bool}) behaves as Complex{Int64}.

@testset "Complex{Int}^Integer (Issue #5768)" begin
    # Integer-component complex to integer powers
    @test (2 + 3im)^2 == -5 + 12im
    @test (1 + 1im)^3 == -2 + 2im
    @test (1 + 2im)^4 == -7 - 24im
    @test (2 + 3im)^0 == 1 + 0im
    @test (2 + 3im)^1 == 2 + 3im

    # `im` is Complex{Bool}; promoted to Complex{Int64}
    @test im^0 == 1 + 0im
    @test im^1 == 0 + 1im
    @test im^2 == -1 + 0im
    @test im^3 == 0 - 1im
    @test im^4 == 1 + 0im

    # Result type is Complex{Int64} (promoted from Bool)
    @test typeof(im^2) == Complex{Int64}
    @test typeof((2 + 3im)^2) == Complex{Int64}
    @test typeof(im^0) == Complex{Int64}

    # Negative power goes through the float complex path
    @test (2 + 3im)^(-1) ≈ (2 - 3im) / 13

    # Float complex powers are unchanged
    @test (2.0 + 3.0im)^2 == -5.0 + 12.0im
end

# Mixed/complex-exponent powers involving integer- or Bool-component complex
# values also hung (no method). They now terminate and agree with the float
# complex result (analytic log/exp, so compared with ≈).
@testset "Complex powers with integer/Bool operands no longer hang (Issue #5768)" begin
    # Real base ^ complex exponent
    @test 2^im ≈ 2.0^im
    @test 2^(1 + 1im) ≈ 2.0^(1.0 + 1.0im)

    # Integer-component complex base ^ complex exponent
    @test (1 + 1im)^im ≈ (1.0 + 1.0im)^im
    @test (1 + 1im)^(1 + 1im) ≈ (1.0 + 1.0im)^(1.0 + 1.0im)
    @test im^(1 + 1im) ≈ (0.0 + 1.0im)^(1.0 + 1.0im)

    # A real-valued complex exponent recovers the base / its power
    @test (2 + 3im)^(1 + 0im) ≈ 2 + 3im
    @test im^(2 + 0im) ≈ -1
end

true
