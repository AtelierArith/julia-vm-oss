using Test

# Issue #3699: comparison-preservation matrix for Float16.
# Crosses {==, <, <=, >, >=} × {Float16 ⊗ Float16, Float16 ⊗ Int64, Float16 ⊗ Float64}
# × {inline, variable-bound}. All comparisons must return Bool.
@testset "Float16 comparison preservation matrix (Issue #3699)" begin
    # ----- == --------------------------------------------------------------
    @test (Float16(1) == Float16(1)) === true
    @test (Float16(1) == Float16(2)) === false
    a = Float16(1); b = Float16(1); c = Float16(2)
    @test (a == b) === true
    @test (a == c) === false
    @test (Float16(1) == 1) === true
    @test (1 == Float16(1)) === true
    @test (Float16(1) == 1.0) === true

    # ----- < ---------------------------------------------------------------
    @test (Float16(1) < Float16(2)) === true
    @test (Float16(2) < Float16(1)) === false
    d = Float16(1); e = Float16(2)
    @test (d < e) === true
    @test (e < d) === false
    @test (Float16(1) < 2) === true
    @test (1 < Float16(2)) === true
    @test (Float16(1) < 2.0) === true

    # ----- <= --------------------------------------------------------------
    @test (Float16(1) <= Float16(1)) === true
    @test (Float16(2) <= Float16(1)) === false
    f = Float16(1); g = Float16(2)
    @test (f <= g) === true
    @test (Float16(1) <= 1) === true

    # ----- > ---------------------------------------------------------------
    @test (Float16(2) > Float16(1)) === true
    @test (Float16(1) > Float16(2)) === false
    h = Float16(2); i = Float16(1)
    @test (h > i) === true
    @test (Float16(2) > 1) === true

    # ----- >= --------------------------------------------------------------
    @test (Float16(2) >= Float16(2)) === true
    @test (Float16(1) >= Float16(2)) === false
    j = Float16(2); k = Float16(2)
    @test (j >= k) === true
    @test (Float16(2) >= 2) === true
end

true
