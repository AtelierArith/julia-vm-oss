using Test

# Issue #3699: type-preservation matrix for Int128 across the binary-op grid.
# Crosses {+, -, *, /, ÷, %} × {Int128 ⊗ Int128, Int128 ⊗ Int64, Int128 ⊗ Float64}
# × {inline-from-constructor, variable-bound}.
#
# Before #3621 / #3694 the inline path leaked through the BigInt early-route or
# the generic div(x, y) = floor(x/y) and silently widened to BigInt / Float64.
# The variable-bound path often happened to work because variable type tracking
# surfaced ValueType::I128 — that asymmetry hid bugs and is why both forms have
# to be tested side-by-side.
@testset "Int128 arithmetic preservation matrix (Issue #3699)" begin
    # ----- + ---------------------------------------------------------------
    @test typeof(Int128(1) + Int128(2)) == Int128
    a = Int128(1); b = Int128(2)
    @test typeof(a + b) == Int128
    @test a + b == Int128(3)

    @test typeof(Int128(1) + 2) == Int128
    @test typeof(2 + Int128(1)) == Int128
    c = Int128(1); d = 2
    @test typeof(c + d) == Int128
    @test typeof(d + c) == Int128

    @test typeof(Int128(1) + 1.0) == Float64
    @test typeof(1.0 + Int128(1)) == Float64
    e = Int128(1); f = 1.0
    @test typeof(e + f) == Float64
    @test typeof(f + e) == Float64

    # ----- - ---------------------------------------------------------------
    @test typeof(Int128(5) - Int128(3)) == Int128
    g = Int128(5); h = Int128(3)
    @test typeof(g - h) == Int128
    @test g - h == Int128(2)

    @test typeof(Int128(5) - 3) == Int128
    @test typeof(5 - Int128(3)) == Int128
    @test typeof(Int128(5) - 3.0) == Float64
    @test typeof(5.0 - Int128(3)) == Float64

    # ----- * ---------------------------------------------------------------
    @test typeof(Int128(3) * Int128(4)) == Int128
    i = Int128(3); j = Int128(4)
    @test typeof(i * j) == Int128
    @test i * j == Int128(12)

    @test typeof(Int128(3) * 4) == Int128
    @test typeof(4 * Int128(3)) == Int128
    @test typeof(Int128(3) * 4.0) == Float64
    @test typeof(4.0 * Int128(3)) == Float64

    # Multiplication that overflows i64 must not wrap to Int64
    big = Int128(typemax(Int64)) * Int128(2)
    @test typeof(big) == Int128
    @test big > Int128(typemax(Int64))

    # ----- / (Julia's `/` always returns Float64 for integer pairs) --------
    @test typeof(Int128(10) / Int128(3)) == Float64
    k = Int128(10); l = Int128(3)
    @test typeof(k / l) == Float64

    @test typeof(Int128(10) / 3) == Float64
    @test typeof(10 / Int128(3)) == Float64
    @test typeof(Int128(10) / 3.0) == Float64

    # ----- ÷ (div) ---------------------------------------------------------
    @test typeof(Int128(10) ÷ Int128(3)) == Int128
    m = Int128(10); n = Int128(3)
    @test typeof(m ÷ n) == Int128
    @test m ÷ n == Int128(3)
    @test typeof(div(Int128(10), Int128(3))) == Int128

    # ÷ across i64::MAX must not silently truncate
    big_val = Int128(typemax(Int64)) * Int128(3)
    @test typeof(big_val ÷ Int128(3)) == Int128
    @test big_val ÷ Int128(3) == Int128(typemax(Int64))

    @test typeof(Int128(10) ÷ 3.0) == Float64
    @test typeof(10.0 ÷ Int128(3)) == Float64

    # ----- % (rem) ---------------------------------------------------------
    @test typeof(Int128(10) % Int128(3)) == Int128
    o = Int128(10); p = Int128(3)
    @test typeof(o % p) == Int128
    @test o % p == Int128(1)
end

true
