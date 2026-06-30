using Test

# Issue #3699: type-preservation matrix for Float16 across the binary-op grid.
# Crosses {+, -, *, /, ÷, %} × {Float16 ⊗ Float16, Float16 ⊗ Int64, Float16 ⊗ Float64}
# × {inline-from-constructor, variable-bound}.
#
# Pre-#3621 the inline `Float16(x) + Float16(y)` widened to Float64 because
# `infer_julia_type` had no `"Float16"` constructor case, so the precise
# type-preservation path in compile/expr/binary never fired. Variable-bound
# Float16 values worked because tracking surfaced the right ValueType.
@testset "Float16 arithmetic preservation matrix (Issue #3699)" begin
    # ----- + ---------------------------------------------------------------
    @test typeof(Float16(1) + Float16(2)) == Float16
    a = Float16(1); b = Float16(2)
    @test typeof(a + b) == Float16
    @test a + b == Float16(3)

    @test typeof(Float16(1) + 2) == Float16
    @test typeof(2 + Float16(1)) == Float16
    c = Float16(1); d = 2
    @test typeof(c + d) == Float16
    @test typeof(d + c) == Float16

    @test typeof(Float16(1) + 1.0) == Float64
    @test typeof(1.0 + Float16(1)) == Float64
    e = Float16(1); f = 1.0
    @test typeof(e + f) == Float64
    @test typeof(f + e) == Float64

    # ----- - ---------------------------------------------------------------
    @test typeof(Float16(5) - Float16(3)) == Float16
    g = Float16(5); h = Float16(3)
    @test typeof(g - h) == Float16
    @test g - h == Float16(2)

    @test typeof(Float16(5) - 3) == Float16
    @test typeof(5 - Float16(3)) == Float16
    @test typeof(Float16(5) - 3.0) == Float64

    # ----- * ---------------------------------------------------------------
    @test typeof(Float16(3) * Float16(4)) == Float16
    i = Float16(3); j = Float16(4)
    @test typeof(i * j) == Float16
    @test i * j == Float16(12)

    @test typeof(Float16(3) * 4) == Float16
    @test typeof(4 * Float16(3)) == Float16
    @test typeof(Float16(3) * 4.0) == Float64

    # ----- / ---------------------------------------------------------------
    @test typeof(Float16(10) / Float16(2)) == Float16
    k = Float16(10); l = Float16(2)
    @test typeof(k / l) == Float16
    @test k / l == Float16(5)

    @test typeof(Float16(10) / 2) == Float16
    @test typeof(10 / Float16(2)) == Float16
    @test typeof(Float16(10) / 2.0) == Float64

    # ----- ÷ (div) ---------------------------------------------------------
    # Float16 ÷ Float16 stays Float16 in Julia (floor of the float quotient).
    @test typeof(Float16(10) ÷ Float16(3)) == Float16
    m = Float16(10); n = Float16(3)
    @test typeof(m ÷ n) == Float16
    @test m ÷ n == Float16(3)

    @test typeof(Float16(10) ÷ 3) == Float16
    @test typeof(Float16(10) ÷ 3.0) == Float64

    # ----- % (rem) ---------------------------------------------------------
    @test typeof(Float16(10) % Float16(3)) == Float16
    o = Float16(10); p = Float16(3)
    @test typeof(o % p) == Float16
    @test o % p == Float16(1)
    @test typeof(Float16(10) % 3) == Float16
end

true
