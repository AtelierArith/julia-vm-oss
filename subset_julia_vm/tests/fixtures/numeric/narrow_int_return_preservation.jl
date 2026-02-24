using Test

# Test that functions returning narrow integer types preserve the original type
# Regression test for Issue #3255 / PR #3270: ReturnI64 now routes narrow ints
# and preserves original type via preserved_val

function get_i8()::Int8
    return Int8(42)
end

function get_i16()::Int16
    return Int16(100)
end

function get_i32()::Int32
    return Int32(-50)
end

function get_i128()::Int128
    return Int128(999)
end

function get_u8()::UInt8
    return UInt8(255)
end

function get_u16()::UInt16
    return UInt16(1000)
end

function get_u32()::UInt32
    return UInt32(42)
end

function get_u64()::UInt64
    return UInt64(100)
end

function get_u128()::UInt128
    return UInt128(9999)
end

function get_bool_true()::Bool
    return true
end

function get_bool_false()::Bool
    return false
end

@testset "narrow integer return type preservation" begin
    @test typeof(get_i8()) == Int8
    @test typeof(get_i16()) == Int16
    @test typeof(get_i32()) == Int32
    @test typeof(get_i128()) == Int128
    @test typeof(get_u8()) == UInt8
    @test typeof(get_u16()) == UInt16
    @test typeof(get_u32()) == UInt32
    @test typeof(get_u64()) == UInt64
    @test typeof(get_u128()) == UInt128
    @test typeof(get_bool_true()) == Bool
    @test typeof(get_bool_false()) == Bool

    # Value correctness
    @test get_i8() == Int8(42)
    @test get_i16() == Int16(100)
    @test get_i32() == Int32(-50)
    @test get_i128() == Int128(999)
    @test get_u8() == UInt8(255)
    @test get_u16() == UInt16(1000)
    @test get_u32() == UInt32(42)
    @test get_u64() == UInt64(100)
    @test get_u128() == UInt128(9999)
    @test get_bool_true() == true
    @test get_bool_false() == false
end

true
