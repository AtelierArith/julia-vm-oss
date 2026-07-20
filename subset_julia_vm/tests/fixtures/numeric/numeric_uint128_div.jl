using Test

# Issue #3696: `÷` on two UInt128s previously fell to the generic
# `div(x, y) = floor(x / y)` (no `div(::UInt128, ::UInt128)` method),
# which widened to Float64. With a Pure Julia specialization backed by a
# U128-aware sdiv_int intrinsic that uses unsigned division, the result
# stays UInt128 — including for values with the high bit set.
@testset "UInt128 div preservation (Issue #3696)" begin
    # Type preservation
    @test typeof(UInt128(10) ÷ UInt128(3)) == UInt128
    @test typeof(div(UInt128(10), UInt128(3))) == UInt128

    # Numerical correctness for small values
    @test UInt128(10) ÷ UInt128(3) == UInt128(3)
    @test div(UInt128(10), UInt128(3)) == UInt128(3)
    @test UInt128(20) ÷ UInt128(7) == UInt128(2)

    # Full-width unsigned (top bit set) — signed semantics would underflow
    @test typeof(UInt128(0xffffffffffffffffffffffffffffffff) ÷ UInt128(3)) == UInt128
    @test UInt128(0xffffffffffffffffffffffffffffffff) ÷ UInt128(3) ==
        UInt128(0x55555555555555555555555555555555)

    # Division by 1 is identity
    @test UInt128(0xffffffffffffffffffffffffffffffff) ÷ UInt128(1) ==
        UInt128(0xffffffffffffffffffffffffffffffff)

    # Division by self is 1
    @test UInt128(0xffffffffffffffffffffffffffffffff) ÷
          UInt128(0xffffffffffffffffffffffffffffffff) == UInt128(1)
end

@testset "UInt128 rem/div with UInt128 divisor (Issue #9770)" begin
    n = typemax(UInt128)
    divisor = UInt128(16)

    @test typeof(rem(n, divisor)) == UInt128
    @test rem(n, divisor) == UInt128(15)
    @test typeof(div(n, divisor)) == UInt128
    @test div(n, divisor) == UInt128(0x0fffffffffffffffffffffffffffffff)
end

true
