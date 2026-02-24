using Test

# Tests that integer-to-integer conversions raise catchable InexactError when
# the value is out of range for the target type (Issue #3063)
# Julia behavior: UInt8(-1), UInt8(256), Int8(-129), Int8(128) etc. all raise InexactError

function uint8_negative_caught()
    caught = false
    try
        x = UInt8(-1)  # InexactError: -1 not representable as UInt8
    catch e
        caught = true
    end
    return caught
end

function uint8_overflow_caught()
    caught = false
    try
        x = UInt8(256)  # InexactError: 256 not representable as UInt8
    catch e
        caught = true
    end
    return caught
end

function int8_overflow_caught()
    caught = false
    try
        x = Int8(128)  # InexactError: 128 not representable as Int8
    catch e
        caught = true
    end
    return caught
end

function int8_underflow_caught()
    caught = false
    try
        x = Int8(-129)  # InexactError: -129 not representable as Int8
    catch e
        caught = true
    end
    return caught
end

function uint8_valid_no_catch()
    # Valid conversions should NOT raise
    caught = false
    try
        x = UInt8(0)
        y = UInt8(255)
        z = UInt8(42)
    catch e
        caught = true
    end
    return caught
end

function int8_valid_no_catch()
    # Valid conversions should NOT raise
    caught = false
    try
        x = Int8(-128)
        y = Int8(127)
        z = Int8(0)
    catch e
        caught = true
    end
    return caught
end

function uint16_overflow_caught()
    caught = false
    try
        x = UInt16(-1)  # InexactError: -1 not representable as UInt16
    catch e
        caught = true
    end
    return caught
end

function int32_overflow_caught()
    caught = false
    try
        x = Int32(2147483648)  # InexactError: 2^31 not representable as Int32
    catch e
        caught = true
    end
    return caught
end

@testset "integer-to-integer InexactError (Issue #3063)" begin
    @test uint8_negative_caught()
    @test uint8_overflow_caught()
    @test int8_overflow_caught()
    @test int8_underflow_caught()
    @test !uint8_valid_no_catch()
    @test !int8_valid_no_catch()
    @test uint16_overflow_caught()
    @test int32_overflow_caught()
end

# Return result so fixture framework can verify all tests passed
(uint8_negative_caught() && uint8_overflow_caught() && int8_overflow_caught() && int8_underflow_caught() && !uint8_valid_no_catch() && !int8_valid_no_catch() && uint16_overflow_caught() && int32_overflow_caught())
