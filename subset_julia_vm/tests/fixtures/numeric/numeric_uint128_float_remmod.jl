using Test

# Issue #9338: rem/mod between UInt128 and Float16/32/64 returned NaN.
#
# The `has_u128_arith && has_float_arith` runtime branch in
# vm/exec/binary_both.rs (Issue #8894) promotes the UInt128 operand to the
# float type for +, -, *, /, and comparisons, but its match had a
# `_ => f64::NAN` default that swallowed `%` (SremInt / `rem`). Because the
# Pure-Julia `mod(x, y)` is derived from `%`, both `rem` and `mod` on any
# UInt128 x float pair returned NaN. `fld`/`cld` were unaffected because they
# derive from `/` (DivFloat), not `%`. Int128 x float already worked (it flows
# through the SremInt float path, which handles I128).
#
# Expected values verified against upstream Julia 1.12.
@testset "UInt128 x float rem/mod (Issue #9338)" begin
    # --- The 4 MWE from the issue ---
    @test mod(Float16(2.5), UInt128(5)) === Float16(2.5)
    @test rem(Float32(2.5), UInt128(5)) === 2.5f0
    @test mod(UInt128(5), Float64(2.5)) === 0.0
    @test rem(UInt128(5), Float16(2.5)) === Float16(0.0)

    # --- Full float-width x UInt128, both operand orders, rem & mod ---
    # Float on the left, UInt128 on the right: result narrows to the float type.
    @test rem(Float16(7.5), UInt128(2)) === Float16(1.5)
    @test mod(Float16(7.5), UInt128(2)) === Float16(1.5)
    @test rem(Float32(7.5), UInt128(2)) === 1.5f0
    @test mod(Float32(7.5), UInt128(2)) === 1.5f0
    @test rem(Float64(7.5), UInt128(2)) === 1.5
    @test mod(Float64(7.5), UInt128(2)) === 1.5

    # UInt128 on the left, float on the right.
    @test rem(UInt128(5), Float16(2.5)) === Float16(0.0)
    @test mod(UInt128(5), Float16(2.5)) === Float16(0.0)
    @test rem(UInt128(5), Float32(2.5)) === 0.0f0
    @test mod(UInt128(5), Float32(2.5)) === 0.0f0
    @test rem(UInt128(5), Float64(2.5)) === 0.0
    @test mod(UInt128(5), Float64(2.5)) === 0.0

    # Negative dividend: rem keeps the sign of the dividend, mod the divisor.
    @test rem(Float64(-7.5), UInt128(2)) === -1.5
    @test mod(Float64(-7.5), UInt128(2)) === 0.5

    # None of the results are NaN (the original bug).
    @test !isnan(rem(Float64(7.5), UInt128(2)))
    @test !isnan(mod(UInt128(5), Float64(2.5)))

    # --- fld/cld stay correct (they were never broken; guard against regressions) ---
    @test fld(Float64(7.5), UInt128(2)) === 3.0
    @test cld(Float64(7.5), UInt128(2)) === 4.0

    # --- Int128 x float parity (was already correct; keep it green) ---
    @test rem(Float64(7.5), Int128(2)) === 1.5
    @test mod(Float64(7.5), Int128(2)) === 1.5
end

true
