using Test

# Issue #8885: BigFloat `repr` must match upstream Julia 1.12 (Base.MPFR)
# bit-for-bit in the last decimal place(s). sjulia's BigFloat is backed by
# astro-float, whose `Display` prints `ceil(prec·log10 2)` significant digits,
# one FEWER than MPFR's `%Re`, which prints `m = 1 + ceil(prec·log10 2)`. The
# binary value is identical; the missing "guard" digit made the last place
# drift (e.g. `true / BigFloat("2.5")` printed `…0001` instead of `…0009`).
# format_bigfloat_julia now reproduces MPFR's digit count (re-render at higher
# precision, round the decimal string to m digits, ties-to-even).
# All expected strings verified against julia 1.12.6 (default 256-bit precision).

@testset "BigFloat mixed-division repr matches MPFR (Issue #8885)" begin
    # The exact MWE from the issue.
    @test repr(true / BigFloat("2.5")) ==
          "0.4000000000000000000000000000000000000000000000000000000000000000000000000000009"

    # Representative mixed-division / reciprocal cells from the #8697 matrix.
    @test repr(BigFloat(1) / 3) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"
    @test repr(BigFloat(2) / 3) ==
          "0.6666666666666666666666666666666666666666666666666666666666666666666666666666695"
    @test repr(BigFloat(1) / 7) ==
          "0.1428571428571428571428571428571428571428571428571428571428571428571428571428568"
    @test repr(BigFloat(10) / BigFloat(3)) ==
          "3.333333333333333333333333333333333333333333333333333333333333333333333333333322"

    # Negative sign is preserved.
    @test repr(-BigFloat(1) / 3) ==
          "-0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"

    # Values that render in scientific `e±NN` form.
    @test repr(BigFloat(1) / BigFloat(1000000)) ==
          "1.000000000000000000000000000000000000000000000000000000000000000000000000000004e-06"
    @test repr(BigFloat(7) / BigFloat(300000)) ==
          "2.333333333333333333333333333333333333333333333333333333333333333333333333333328e-05"

    # sqrt result (full 256-bit mantissa).
    @test repr(sqrt(BigFloat(2))) ==
          "1.414213562373095048801688724209698078569671875376948073176679737990732478462102"

    # Round-half-to-even carry-out case: value just below 1 rounds up in the
    # guard digit without spuriously becoming 1.0.
    @test repr(BigFloat("0.99999999999999999999999999999999999999999999999999999999999999999999999999999")) ==
          "0.9999999999999999999999999999999999999999999999999999999999999999999999999999914"

    # Regression guard: terminating short-dyadic values (from Float64) whose
    # exact decimal expansion is shorter than m digits must stay unchanged.
    @test repr(big(0.1)) ==
          "0.1000000000000000055511151231257827021181583404541015625"
    @test repr(BigFloat("2.5")) == "2.5"
end

true
