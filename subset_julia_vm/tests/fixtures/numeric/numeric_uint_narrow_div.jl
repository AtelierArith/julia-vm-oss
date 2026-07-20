using Test

# Issue #3701: UInt8/UInt16/UInt32/UInt64 ÷ previously fell to the generic
# `div(x, y) = floor(x / y)` and widened to Float64. Pure Julia fixed-width
# dispatch keeps the result as UIntN — including for UInt64 values above
# i64::MAX.
@testset "Narrow UInt div preservation (Issue #3701)" begin
    # Type preservation
    @test typeof(UInt8(10) ÷ UInt8(3)) == UInt8
    @test typeof(UInt16(10) ÷ UInt16(3)) == UInt16
    @test typeof(UInt32(10) ÷ UInt32(3)) == UInt32
    @test typeof(UInt64(10) ÷ UInt64(3)) == UInt64

    @test typeof(div(UInt8(10), UInt8(3))) == UInt8
    @test typeof(div(UInt16(10), UInt16(3))) == UInt16
    @test typeof(div(UInt32(10), UInt32(3))) == UInt32
    @test typeof(div(UInt64(10), UInt64(3))) == UInt64

    # Numerical correctness
    @test UInt8(10) ÷ UInt8(3) == UInt8(3)
    @test UInt16(10) ÷ UInt16(3) == UInt16(3)
    @test UInt32(10) ÷ UInt32(3) == UInt32(3)
    @test UInt64(10) ÷ UInt64(3) == UInt64(3)

    # UInt8 full-range value (avoid `typemax(UInt8)` until #3702 is fixed)
    @test UInt8(255) ÷ UInt8(3) == UInt8(85)
    @test UInt16(0xffff) ÷ UInt16(3) == UInt16(0x5555)
    @test UInt32(0xffffffff) ÷ UInt32(3) == UInt32(0x55555555)

    # UInt64 above i64::MAX — previously raised OverflowError or wrapped
    @test UInt64(0xffffffffffffffff) ÷ UInt64(3) == UInt64(0x5555555555555555)
    @test UInt64(0xffffffffffffffff) ÷ UInt64(2) == UInt64(0x7fffffffffffffff)
    @test typeof(UInt64(0xffffffffffffffff) ÷ UInt64(2)) == UInt64

    # Division by 1 is identity (across widths)
    @test UInt8(255) ÷ UInt8(1) == UInt8(255)
    @test UInt64(0xffffffffffffffff) ÷ UInt64(1) == UInt64(0xffffffffffffffff)
end

true
