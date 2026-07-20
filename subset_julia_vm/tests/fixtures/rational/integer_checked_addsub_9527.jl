# Issue #9527: +/- between a Rational and an Integer cross-multiply the
# numerator with a plain (wrapping) `*`/`+`/`-`, so an operand whose product or
# sum overflows the element type silently wrapped into a huge Rational instead
# of raising upstream's catchable OverflowError. Upstream
# julia/base/rational.jl forms the numerator with checked_mul/checked_add/
# checked_sub (base/checked.jl); sjulia now ports checked_add/checked_sub
# (checked_mul was ported for #9416/#9422) and uses them here.
#
# Note: the unsigned OverflowError message text still prints operands in hex
# (display divergence #9374) — that is out of scope; this fixture asserts on the
# thrown exception type and on the non-overflowing values, not on the message.
#
# All expected values/types verified against upstream julia 1.12.

using Test

@testset "Rational +/- Integer overflows raise OverflowError (Issue #9527)" begin
    # 3//4 - UInt64(5): cross-multiplied numerator 3 - 20 underflows UInt64.
    @test_throws OverflowError 3 // 4 - UInt64(5)

    # An ordinary try/catch must be able to catch it.
    caught = try
        3 // 4 - UInt64(5)
        false
    catch e
        e isa OverflowError
    end
    @test caught

    # Signed overflow on the checked_add / checked_sub of the numerator.
    @test_throws OverflowError Rational{Int8}(127, 1) + Int8(1)
    @test_throws OverflowError Int8(1) + Rational{Int8}(127, 1)
    @test_throws OverflowError Rational{Int8}(-128, 1) - Int8(1)
end

@testset "Rational +/- Integer non-overflowing values unchanged (Issue #9527)" begin
    # Value ok even though unsigned Rational display is #9374.
    @test (3 // 4 + UInt64(5)) == 23 // 4
    @test typeof(3 // 4 + UInt64(5)) === Rational{UInt64}
    @test (UInt64(5) - 3 // 4) == 17 // 4
    @test typeof(UInt64(5) - 3 // 4) === Rational{UInt64}

    @test 3 // 4 + 2 === 11 // 4
    @test 2 + 3 // 4 === 11 // 4
    @test 5 - 1 // 2 === 9 // 2
    @test 2 // 3 - 5 === -13 // 3

    @test Rational{Int8}(3, 4) + Int8(1) === Rational{Int8}(7, 4)
    @test Int8(-1) - Rational{Int8}(127, 1) === Rational{Int8}(-128, 1)
end

true
