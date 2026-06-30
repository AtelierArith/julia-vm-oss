using Test

# Issue #3621: Inline `Float16(x) + Float16(y)` previously inferred to Any
# (since type inference was missing the Float16 constructor) and routed
# through CallDynamicBinaryBoth, which lacked a `both_f16` case and
# returned Float64. This file pins the inline path along with mixed
# Float16+other arithmetic.
@testset "Float16 inline type preservation (Issue #3621)" begin
    # Inline arithmetic preserves Float16
    @test typeof(Float16(1.0) + Float16(2.0)) == Float16
    @test typeof(Float16(3.0) - Float16(1.0)) == Float16
    @test typeof(Float16(2.0) * Float16(3.0)) == Float16
    @test typeof(Float16(6.0) / Float16(2.0)) == Float16
    @test Float16(1.0) + Float16(2.0) == Float16(3.0)

    # Variable-bound arithmetic also preserves Float16
    x = Float16(1.5)
    y = Float16(2.5)
    @test typeof(x + y) == Float16
    @test typeof(x * y) == Float16

    # Float16 + Int promotes to Float16 (smaller numeric is widened to F16)
    @test typeof(Float16(1.0) + 1) == Float16
    @test typeof(1 + Float16(1.0)) == Float16

    # Float16 + Float64 promotes to Float64 (larger float wins)
    @test typeof(Float16(1.0) + 1.0) == Float64
    @test typeof(1.0 + Float16(1.0)) == Float64

    # Comparisons return Bool
    @test (Float16(1.0) < Float16(2.0)) === true
    @test (Float16(2.0) >= Float16(2.0)) === true
    @test (Float16(1.0) == Float16(1.0)) === true
end

true
