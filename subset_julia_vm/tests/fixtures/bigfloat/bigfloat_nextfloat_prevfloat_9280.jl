using Test

# Issue #9280: nextfloat / prevfloat on a BigFloat previously errored with
# "reinterpret(Int64, BigFloat): size mismatch" because the value fell through
# to the generic `nextfloat(x::T) where {T<:AbstractFloat}` in base/float.jl,
# which steps the Float64 bit pattern via `reinterpret`. BigFloat now has its
# own methods (base/gmp.jl) that advance by one ULP at the value's precision,
# mirroring MPFR's mpfr_nextabove / mpfr_nextbelow (implemented over the
# astro-float backend). All expected values verified against julia 1.12.6 at
# the default 256-bit precision.

@testset "nextfloat/prevfloat for BigFloat (Issue #9280)" begin
    # ---- positive value: one ULP up/down, full-digit repr matches MPFR ----
    x = BigFloat("1.5")
    @test string(nextfloat(x)) ==
          "1.500000000000000000000000000000000000000000000000000000000000000000000000000017"
    @test string(prevfloat(x)) ==
          "1.499999999999999999999999999999999999999999999999999999999999999999999999999983"
    @test nextfloat(x) > x
    @test prevfloat(x) < x
    # nextfloat(x) - x is exactly one ULP = 2^(exponent-precision+1) = 2^-255.
    @test nextfloat(x) == x + ldexp(BigFloat("1.0"), -255)
    @test prevfloat(x) == x - ldexp(BigFloat("1.0"), -255)

    # ---- negative value: nextfloat moves toward zero ----
    y = BigFloat("-2.5")
    @test string(nextfloat(y)) ==
          "-2.499999999999999999999999999999999999999999999999999999999999999999999999999965"
    @test string(prevfloat(y)) ==
          "-2.500000000000000000000000000000000000000000000000000000000000000000000000000035"
    @test nextfloat(y) > y
    @test prevfloat(y) < y

    # ---- exact power of two: the ULP halves below the boundary ----
    p2 = BigFloat("1.0")
    @test string(nextfloat(p2)) ==
          "1.000000000000000000000000000000000000000000000000000000000000000000000000000017"
    @test string(prevfloat(p2)) ==
          "0.9999999999999999999999999999999999999999999999999999999999999999999999999999914"
    # Above 1.0 the ULP is 2^-255; below it (smaller binade) the ULP is 2^-256.
    @test nextfloat(p2) == p2 + ldexp(BigFloat("1.0"), -255)
    @test prevfloat(p2) == p2 - ldexp(BigFloat("1.0"), -256)

    # ---- zero: steps to the smallest ± value; symmetric; round-trips to 0 ----
    z = BigFloat("0.0")
    @test nextfloat(z) > 0
    @test prevfloat(z) < 0
    @test nextfloat(z) == -prevfloat(z)
    @test prevfloat(nextfloat(z)) == z
    @test nextfloat(prevfloat(z)) == z

    # ---- round-trip: prevfloat(nextfloat(x)) == x for positive and negative ----
    @test prevfloat(nextfloat(x)) == x
    @test nextfloat(prevfloat(x)) == x
    @test prevfloat(nextfloat(y)) == y
    @test nextfloat(prevfloat(y)) == y

    # ---- the (x, n) arity matches iterated single steps and upstream ----
    @test string(nextfloat(x, 3)) ==
          "1.500000000000000000000000000000000000000000000000000000000000000000000000000052"
    @test nextfloat(x, 3) == nextfloat(nextfloat(nextfloat(x)))
    @test nextfloat(x, 0) == x
    @test prevfloat(x, 0) == x
    @test nextfloat(x, -2) == prevfloat(prevfloat(x))
    @test prevfloat(x, 2) == prevfloat(prevfloat(x))
    @test prevfloat(x, -3) == nextfloat(nextfloat(nextfloat(x)))
    # nextfloat(x, -1) is exactly prevfloat(x); prevfloat(x, -1) is nextfloat(x).
    @test nextfloat(x, -1) == prevfloat(x)
    @test prevfloat(x, -1) == nextfloat(x)

    # ---- ±Inf / NaN edge behaviour matches upstream ----
    @test nextfloat(BigFloat(Inf)) == BigFloat(Inf)
    @test prevfloat(BigFloat(-Inf)) == BigFloat(-Inf)
    @test prevfloat(BigFloat(Inf)) < BigFloat(Inf)
    @test nextfloat(BigFloat(-Inf)) > BigFloat(-Inf)
    @test isnan(nextfloat(BigFloat(NaN)))
    @test isnan(prevfloat(BigFloat(NaN)))
end

true
