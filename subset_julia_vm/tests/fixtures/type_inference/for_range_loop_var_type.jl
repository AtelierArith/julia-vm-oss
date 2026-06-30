# Test: range-based `for` loop variable type follows range element promotion
# rather than being hard-coded to Int64 (Issue #3518).

using Test

# Float step range: loop variable should be Float64.
function f_float_step()
    last = 0.0
    for x in 1.0:0.5:2.0
        last = x
    end
    last
end

# Integer range still infers Int64 (regression guard).
function f_int_range()
    last = 0
    for x in 1:3
        last = x
    end
    last
end

# UInt8 range keeps UInt8 element type.
function f_uint8_range()
    last = UInt8(0)
    for x in UInt8(1):UInt8(3)
        last = x
    end
    last
end

@testset "for-range loop variable type (Issue #3518)" begin
    @test f_float_step() == 2.0
    @test f_float_step() isa Float64

    @test f_int_range() == 3
    @test f_int_range() isa Int

    @test f_uint8_range() == UInt8(3)
    @test f_uint8_range() isa UInt8
end

true  # Test passed
