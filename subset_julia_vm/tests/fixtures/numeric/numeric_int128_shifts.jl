using Test

# Issue #3747: 128-bit shift operators (<<, >>, >>>) used to raise
# `Dispatch(NoMethodFound)` because the pure-Julia layer in
# subset_julia_vm/src/julia/base/int.jl was missing the UInt128 specializations
# (Int128 was added by PR #3565, UInt128 was forgotten). Runtime intrinsics
# `ShlInt`/`LshrInt`/`AshrInt` already preserve `UInt128`/`Int128` width.
@testset "128-bit shift operators (Issue #3747)" begin
    # ===== UInt128 =====
    # Logical left shift: UInt128 << Int -> UInt128
    @test typeof(UInt128(1) << 1) == UInt128
    @test UInt128(1) << 1 == UInt128(2)
    @test UInt128(1) << 4 == UInt128(16)

    # Logical right shift (>>): UInt128 has no sign bit, so >> == >>>
    @test typeof(UInt128(8) >> 1) == UInt128
    @test UInt128(8) >> 1 == UInt128(4)
    @test typeof(UInt128(8) >>> 1) == UInt128
    @test UInt128(8) >>> 1 == UInt128(4)

    # Large UInt128 shift correctness — value exceeds UInt64 range
    big_u = UInt128(typemax(UInt64)) + UInt128(1)  # = 1 << 64
    @test big_u >> 1 == UInt128(1) << 63
    @test big_u >>> 64 == UInt128(1)
    @test UInt128(1) << 64 == big_u

    # ===== Int128 =====
    # Already worked before this fix, but lock it in to prevent regression.
    @test typeof(Int128(1) << 1) == Int128
    @test Int128(1) << 1 == Int128(2)

    # Arithmetic right shift preserves sign for Int128
    @test typeof(Int128(8) >> 1) == Int128
    @test Int128(8) >> 1 == Int128(4)
    @test Int128(-8) >> 1 == Int128(-4)

    # Logical right shift fills with zeros
    @test typeof(Int128(-8) >>> 1) == Int128
    @test Int128(-8) >>> 1 == Int128(170141183460469231731687303715884105724)

    # ===== Variable-bound paths =====
    a = UInt128(7)
    @test typeof(a << 2) == UInt128
    @test a << 2 == UInt128(28)

    b = Int128(7)
    @test typeof(b << 2) == Int128
    @test b << 2 == Int128(28)
end

true
