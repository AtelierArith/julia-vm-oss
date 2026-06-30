using Test

@testset "convert UInt widening preserves arithmetic-dispatch type (Issue #6755)" begin
    # The core bug: convert(UInt64, ::UInt32) produced a value whose typeof was
    # UInt64 but whose internal tag was Int64, so the following `*` dispatched to Int64.
    @test typeof(convert(UInt64, UInt32(5)) * UInt64(2)) === UInt64

    # Through a binding (the original repro).
    v = convert(UInt64, UInt32(5))
    @test typeof(v) === UInt64
    @test typeof(v * UInt64(2)) === UInt64
    @test typeof(UInt64(2) * v) === UInt64
    @test v * UInt64(2) === UInt64(10)

    # All narrow-unsigned -> wider-unsigned conversions preserve the type under arithmetic.
    @test typeof(convert(UInt16, UInt8(5)) * UInt16(2)) === UInt16
    @test typeof(convert(UInt32, UInt16(5)) * UInt32(2)) === UInt32
    @test typeof(convert(UInt64, UInt32(5)) * UInt64(2)) === UInt64
    @test typeof(convert(UInt128, UInt64(5)) * UInt128(2)) === UInt128

    # signed -> unsigned conversions, then arithmetic.
    @test typeof(convert(UInt64, Int64(5)) * UInt64(2)) === UInt64
    @test typeof(convert(UInt32, Int32(5)) * UInt32(2)) === UInt32

    # Full set of arithmetic ops on converted UInt64 keep the UInt64 tag.
    a = convert(UInt64, UInt32(7))
    b = UInt64(3)
    @test typeof(a + b) === UInt64
    @test a + b === UInt64(10)
    @test typeof(a - b) === UInt64
    @test a - b === UInt64(4)
    @test typeof(a * b) === UInt64
    @test a * b === UInt64(21)
    @test typeof(a ÷ b) === UInt64
    @test a ÷ b === UInt64(2)
    @test typeof(a % b) === UInt64
    @test a % b === UInt64(1)
    @test typeof(a / b) === Float64

    # Wrapping subtraction (result above i64::MAX) stays UInt64.
    @test typeof(UInt64(1) - UInt64(3)) === UInt64
    @test UInt64(1) - UInt64(3) === UInt64(18446744073709551614)

    # Mixed UInt64 + small Int promotes the Int and keeps UInt64.
    @test typeof(a + 3) === UInt64
    @test a + 3 === UInt64(10)
    @test typeof(a * 2) === UInt64
    @test a * 2 === UInt64(14)
end

true
