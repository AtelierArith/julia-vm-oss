using Test

# Issue #8894: UInt128 typemax mixed with narrow floats exits during conversion.
# typemax(UInt128) + Float32/Float64 should produce Float64/Float32 without crashing.

let
    x = typemax(UInt128)
    @test typeof(x) == UInt128
    @test x == 340282366920938463463374607431768211455

    # UInt128 + Float64
    @test typeof(x + 1.0) == Float64
    @test isfinite(x + 1.0)   # large but finite

    # UInt128 + Float32 — overflows Float32 range → Inf32
    @test typeof(x + Float32(1.0)) == Float32
    @test isinf(x + Float32(1.0))

    # UInt128 / Float64
    @test typeof(x / 1.0) == Float64
    @test x / 1.0 > 0

    # Convert UInt128 to Float32
    r = Float32(typemax(UInt128))
    @test typeof(r) == Float32
    @test r > Float32(0)
end

true
