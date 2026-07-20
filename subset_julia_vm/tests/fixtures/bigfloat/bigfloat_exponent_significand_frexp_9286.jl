using Test

# Issue #9286: exponent / significand / frexp on a BigFloat previously errored
# with "reinterpret(UInt64, BigFloat): size mismatch" because the value fell
# through to the generic `where {T<:AbstractFloat}` definitions in
# base/float.jl, which decode the Float64 bit pattern via `reinterpret`.
# BigFloat now has its own methods (base/gmp.jl) built on the astro-float
# exponent (mirroring MPFR's mpfr_get_exp / mpfr_frexp): for a finite nonzero x,
# `x = m·2^E` with `m ∈ [0.5, 1)`, so `frexp` returns `(m, E)`, `exponent` is
# `E - 1` (Base's [1, 2) convention) and `significand` is `m·2` (in [1, 2)).
# All expected values verified against julia 1.12.6.

@testset "exponent/significand/frexp for BigFloat (Issue #9286)" begin
    # ---- exponent: unbiased base-2 exponent (Base [1,2) convention) ----
    @test exponent(BigFloat("1.5")) == 0
    @test exponent(BigFloat("0.75")) == -1
    @test exponent(BigFloat("-1.5")) == 0     # sign-independent
    @test exponent(BigFloat("2.0")) == 1
    @test exponent(BigFloat("0.5")) == -1
    @test exponent(BigFloat("1024.0")) == 10
    @test exponent(BigFloat("-0.25")) == -2
    @test exponent(BigFloat("3.0")) == 1
    @test exponent(BigFloat("1e-40")) == -133
    @test exponent(BigFloat("1.5")) isa Int

    # ---- significand: x normalized to [1, 2) keeping its sign ----
    @test significand(BigFloat("1.5")) == BigFloat("1.5")
    @test significand(BigFloat("0.75")) == BigFloat("1.5")
    @test significand(BigFloat("-1.5")) == BigFloat("-1.5")
    @test significand(BigFloat("2.0")) == BigFloat("1.0")
    @test significand(BigFloat("0.5")) == BigFloat("1.0")
    @test significand(BigFloat("1024.0")) == BigFloat("1.0")
    @test significand(BigFloat("-0.25")) == BigFloat("-1.0")
    @test significand(BigFloat("3.0")) == BigFloat("1.5")
    @test significand(BigFloat("1.5")) isa BigFloat
    # significand is in [1, 2) for a positive value
    let s = significand(BigFloat("0.1"))
        @test s >= BigFloat("1.0")
        @test s < BigFloat("2.0")
    end

    # ---- frexp: (m, E) with m ∈ [0.5, 1) keeping sign, and x == m·2^E ----
    @test frexp(BigFloat("1.5")) == (BigFloat("0.75"), 1)
    @test frexp(BigFloat("0.75")) == (BigFloat("0.75"), 0)
    @test frexp(BigFloat("-1.5")) == (BigFloat("-0.75"), 1)
    @test frexp(BigFloat("2.0")) == (BigFloat("0.5"), 2)
    @test frexp(BigFloat("0.5")) == (BigFloat("0.5"), 0)
    @test frexp(BigFloat("1024.0")) == (BigFloat("0.5"), 11)
    @test frexp(BigFloat("-0.25")) == (BigFloat("-0.5"), -1)
    @test frexp(BigFloat("3.0")) == (BigFloat("0.75"), 2)
    let (m, e) = frexp(BigFloat("0.1"))
        @test m isa BigFloat
        @test e isa Int
        # mantissa in [0.5, 1) and exact reconstruction
        @test m >= BigFloat("0.5")
        @test m < BigFloat("1.0")
        @test ldexp(m, e) == BigFloat("0.1")
    end

    # ---- cross-checks between the three functions on a non-dyadic value ----
    let x = BigFloat("0.1")
        m, e = frexp(x)
        @test e == exponent(x) + 1          # frexp exponent is exponent+1
        @test significand(x) == m * 2       # significand doubles the frexp mantissa
    end

    # ---- exponent throws DomainError for ±0, ±Inf, NaN (matches upstream) ----
    @test_throws DomainError exponent(BigFloat(0))
    @test_throws DomainError exponent(BigFloat(Inf))
    @test_throws DomainError exponent(BigFloat(-Inf))
    @test_throws DomainError exponent(BigFloat(NaN))

    # ---- significand returns x unchanged for ±0, ±Inf, NaN ----
    @test significand(BigFloat(0)) == BigFloat(0)
    @test significand(BigFloat(Inf)) == BigFloat(Inf)
    @test significand(BigFloat(-Inf)) == BigFloat(-Inf)
    @test isnan(significand(BigFloat(NaN)))

    # ---- frexp returns (x, 0) for ±0, ±Inf, NaN ----
    @test frexp(BigFloat(0)) == (BigFloat(0), 0)
    @test frexp(BigFloat(Inf)) == (BigFloat(Inf), 0)
    @test frexp(BigFloat(-Inf)) == (BigFloat(-Inf), 0)
    let (m, e) = frexp(BigFloat(NaN))
        @test isnan(m)
        @test e == 0
    end
end

true
