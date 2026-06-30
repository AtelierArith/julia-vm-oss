using Test

# Issue #3699: type-preservation matrix for UInt128 across the binary-op grid.
# Crosses {+, -, *, /, ÷, %} × {UInt128 ⊗ UInt128, UInt128 ⊗ Int64, UInt128 ⊗ Float64}
# × {inline, variable-bound}.
#
# UInt128 has no compile-time BigInt early-route to fall through, so the
# pre-#3697 default was the I64 path which silently truncated; pre-#3696 the
# division path widened to Float64.
@testset "UInt128 arithmetic preservation matrix (Issue #3699)" begin
    # ----- + ---------------------------------------------------------------
    @test typeof(UInt128(1) + UInt128(2)) == UInt128
    a = UInt128(1); b = UInt128(2)
    @test typeof(a + b) == UInt128
    @test a + b == UInt128(3)

    @test typeof(UInt128(1) + 2) == UInt128
    @test typeof(2 + UInt128(1)) == UInt128
    c = UInt128(1); d = 2
    @test typeof(c + d) == UInt128
    @test typeof(d + c) == UInt128

    @test typeof(UInt128(1) + 1.0) == Float64
    @test typeof(1.0 + UInt128(1)) == Float64

    # ----- - ---------------------------------------------------------------
    @test typeof(UInt128(5) - UInt128(2)) == UInt128
    e = UInt128(5); f = UInt128(2)
    @test typeof(e - f) == UInt128
    @test e - f == UInt128(3)

    @test typeof(UInt128(5) - 2) == UInt128
    @test typeof(UInt128(5) - 2.0) == Float64

    # ----- * ---------------------------------------------------------------
    @test typeof(UInt128(3) * UInt128(4)) == UInt128
    g = UInt128(3); h = UInt128(4)
    @test typeof(g * h) == UInt128
    @test g * h == UInt128(12)

    @test typeof(UInt128(3) * 4) == UInt128
    @test typeof(4 * UInt128(3)) == UInt128
    @test typeof(UInt128(3) * 4.0) == Float64

    # Multiplication that overflows u64 must NOT truncate to Int64 (Issue #3697)
    big_u = UInt128(typemax(UInt64)) * UInt128(2)
    @test typeof(big_u) == UInt128
    @test big_u > UInt128(typemax(UInt64))

    # ----- / (always Float64) ---------------------------------------------
    @test typeof(UInt128(10) / UInt128(3)) == Float64
    i = UInt128(10); j = UInt128(3)
    @test typeof(i / j) == Float64

    @test typeof(UInt128(10) / 3) == Float64
    @test typeof(UInt128(10) / 3.0) == Float64

    # ----- ÷ (div) ---------------------------------------------------------
    @test typeof(UInt128(10) ÷ UInt128(3)) == UInt128
    k = UInt128(10); l = UInt128(3)
    @test typeof(k ÷ l) == UInt128
    @test k ÷ l == UInt128(3)
    @test typeof(div(UInt128(10), UInt128(3))) == UInt128

    # ÷ at full UInt128 width (Issue #3696)
    big_val = UInt128(typemax(UInt64)) * UInt128(3)
    @test typeof(big_val ÷ UInt128(3)) == UInt128
    @test big_val ÷ UInt128(3) == UInt128(typemax(UInt64))

    # ----- % (rem) ---------------------------------------------------------
    @test typeof(UInt128(10) % UInt128(3)) == UInt128
    m = UInt128(10); n = UInt128(3)
    @test typeof(m % n) == UInt128
    @test m % n == UInt128(1)
end

true
