using Test

# Issue #3699: comparison-preservation matrix for UInt128.
# Crosses {==, <, <=, >, >=} × {UInt128 ⊗ UInt128, UInt128 ⊗ Int64, UInt128 ⊗ Float64}
# × {inline, variable-bound}.
#
# Key case (called out explicitly in the issue): UInt128 comparisons at full
# width (above i64::MAX) used to raise OverflowError because the runtime
# fallback's small-int prologue did try_from(U128 → I64).
@testset "UInt128 comparison preservation matrix (Issue #3699)" begin
    # ----- == --------------------------------------------------------------
    @test (UInt128(1) == UInt128(1)) === true
    @test (UInt128(1) == UInt128(2)) === false
    a = UInt128(1); b = UInt128(1); c = UInt128(2)
    @test (a == b) === true
    @test (a == c) === false
    @test (UInt128(1) == 1) === true
    @test (1 == UInt128(1)) === true
    @test (UInt128(1) == 1.0) === true

    # ----- < ---------------------------------------------------------------
    @test (UInt128(1) < UInt128(2)) === true
    @test (UInt128(2) < UInt128(1)) === false
    d = UInt128(1); e = UInt128(2)
    @test (d < e) === true
    @test (e < d) === false
    @test (UInt128(1) < 2) === true
    @test (1 < UInt128(2)) === true
    @test (UInt128(1) < 2.0) === true

    # ----- <= --------------------------------------------------------------
    @test (UInt128(1) <= UInt128(1)) === true
    @test (UInt128(2) <= UInt128(1)) === false
    f = UInt128(1); g = UInt128(2)
    @test (f <= g) === true
    @test (UInt128(1) <= 1) === true

    # ----- > ---------------------------------------------------------------
    @test (UInt128(2) > UInt128(1)) === true
    @test (UInt128(1) > UInt128(2)) === false
    h = UInt128(2); i = UInt128(1)
    @test (h > i) === true
    @test (UInt128(2) > 1) === true

    # ----- >= --------------------------------------------------------------
    @test (UInt128(2) >= UInt128(2)) === true
    @test (UInt128(1) >= UInt128(2)) === false
    j = UInt128(2); k = UInt128(2)
    @test (j >= k) === true
    @test (UInt128(2) >= 2) === true

    # ----- Comparisons at full UInt128 width (above i64::MAX) --------------
    # Issue #3696: this exact case used to raise OverflowError.
    big_u_a = UInt128(typemax(UInt64)) * UInt128(2)
    big_u_b = UInt128(typemax(UInt64)) * UInt128(2)
    big_u_c = UInt128(typemax(UInt64)) * UInt128(2) + UInt128(1)
    @test (big_u_a == big_u_b) === true
    @test (big_u_a < big_u_c) === true
    @test (big_u_c > big_u_a) === true
    @test (big_u_a <= big_u_b) === true
    @test (big_u_c >= big_u_a) === true

    # Above-i64::MAX comparison via inline constructor (no variables)
    @test (UInt128(typemax(UInt64)) * UInt128(2) ==
           UInt128(typemax(UInt64)) * UInt128(2)) === true
    @test (UInt128(typemax(UInt64)) * UInt128(2) <
           UInt128(typemax(UInt64)) * UInt128(2) + UInt128(1)) === true
end

true
