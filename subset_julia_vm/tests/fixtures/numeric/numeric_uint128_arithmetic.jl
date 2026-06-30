using Test

# Issue #3697: UInt128 arithmetic (+, -, *, %) used to fall through to the
# I64 generic path and silently truncate or raise OverflowError. The new
# UInt128 early-route in compile/expr/binary/mod.rs and the parallel
# `has_u128_arith` runtime branch in vm/exec/binary_both.rs preserve
# UInt128 (or promote to the appropriate float when mixed with F16/F32/F64).
@testset "UInt128 arithmetic preservation (Issue #3697)" begin
    # Same-type arithmetic preserves UInt128
    @test typeof(UInt128(1) + UInt128(2)) == UInt128
    @test typeof(UInt128(5) - UInt128(2)) == UInt128
    @test typeof(UInt128(3) * UInt128(4)) == UInt128
    @test typeof(UInt128(10) % UInt128(3)) == UInt128
    @test UInt128(1) + UInt128(2) == UInt128(3)
    @test UInt128(5) - UInt128(2) == UInt128(3)
    @test UInt128(3) * UInt128(4) == UInt128(12)

    # `/` on UInt128 always returns Float64 (Julia's integer division rule)
    @test typeof(UInt128(10) / UInt128(3)) == Float64

    # Variable-bound paths
    a = UInt128(7)
    b = UInt128(3)
    @test typeof(a + b) == UInt128
    @test typeof(a * b) == UInt128
    @test a + b == UInt128(10)
    @test a * b == UInt128(21)

    # Multiplication that overflows UInt64 must NOT truncate to Int64
    big_u = UInt128(typemax(UInt64)) * UInt128(2)
    @test typeof(big_u) == UInt128
    @test big_u == UInt128(0x1fffffffffffffffe)

    # Mixed UInt128 + Int64 stays UInt128 (signed promoted to unsigned)
    @test typeof(UInt128(1) + 2) == UInt128
    @test typeof(2 + UInt128(1)) == UInt128
    @test UInt128(1) + 2 == UInt128(3)

    # Mixed UInt128 + Float promotes to that float type
    @test typeof(UInt128(1) + 1.0) == Float64
    @test typeof(UInt128(1) + Float32(1.0)) == Float32
    @test typeof(UInt128(1) + Float16(1.0)) == Float16

    # Comparisons return Bool (handled by the U64/U128 cmp early-route from #3696)
    @test (UInt128(1) < UInt128(2)) === true
    @test (UInt128(1) == UInt128(1)) === true
    @test (UInt128(2) > UInt128(1)) === true
end

true
