using Test

# Issue #3699: comparison-preservation matrix for Int128.
# Crosses {==, <, <=, >, >=} × {Int128 ⊗ Int128, Int128 ⊗ Int64, Int128 ⊗ Float64}
# × {inline, variable-bound}. All comparisons must return Bool.
@testset "Int128 comparison preservation matrix (Issue #3699)" begin
    # Each cell of the grid for {==, <, <=, >, >=} —
    # inline AND variable-bound, both i128⊗i128, i128⊗i64, i128⊗f64.

    # ----- == --------------------------------------------------------------
    @test (Int128(1) == Int128(1)) === true
    @test (Int128(1) == Int128(2)) === false
    a = Int128(1); b = Int128(1); c = Int128(2)
    @test (a == b) === true
    @test (a == c) === false
    @test (Int128(1) == 1) === true
    @test (1 == Int128(1)) === true
    @test (Int128(1) == 1.0) === true
    @test (1.0 == Int128(1)) === true

    # ----- < ---------------------------------------------------------------
    @test (Int128(1) < Int128(2)) === true
    @test (Int128(2) < Int128(1)) === false
    d = Int128(1); e = Int128(2)
    @test (d < e) === true
    @test (e < d) === false
    @test (Int128(1) < 2) === true
    @test (1 < Int128(2)) === true
    @test (Int128(1) < 2.0) === true
    @test (1.0 < Int128(2)) === true

    # ----- <= --------------------------------------------------------------
    @test (Int128(1) <= Int128(1)) === true
    @test (Int128(2) <= Int128(1)) === false
    f = Int128(1); g = Int128(2)
    @test (f <= g) === true
    @test (g <= f) === false
    @test (Int128(1) <= 1) === true
    @test (1 <= Int128(1)) === true
    @test (Int128(1) <= 1.0) === true

    # ----- > ---------------------------------------------------------------
    @test (Int128(2) > Int128(1)) === true
    @test (Int128(1) > Int128(2)) === false
    h = Int128(2); i = Int128(1)
    @test (h > i) === true
    @test (i > h) === false
    @test (Int128(2) > 1) === true
    @test (2 > Int128(1)) === true
    @test (Int128(2) > 1.0) === true

    # ----- >= --------------------------------------------------------------
    @test (Int128(2) >= Int128(2)) === true
    @test (Int128(1) >= Int128(2)) === false
    j = Int128(2); k = Int128(2)
    @test (j >= k) === true
    @test (Int128(2) >= 2) === true
    @test (2 >= Int128(2)) === true
    @test (Int128(2) >= 2.0) === true

    # ----- Comparisons at full Int128 width (above i64::MAX) ---------------
    # The runtime fallback's small-int prologue used to try_from(I128 → I64),
    # raising OverflowError. These comparisons must succeed and return Bool.
    big_a = Int128(typemax(Int64)) * Int128(2)
    big_b = Int128(typemax(Int64)) * Int128(2)
    big_c = Int128(typemax(Int64)) * Int128(2) + Int128(1)
    @test (big_a == big_b) === true
    @test (big_a < big_c) === true
    @test (big_c > big_a) === true
    @test (big_a <= big_b) === true
    @test (big_c >= big_a) === true
end

true
