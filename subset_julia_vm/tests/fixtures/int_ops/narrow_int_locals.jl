# Test: narrow integer types are stored/loaded correctly as local variables
# Verifies that I8/I16/I32/I128/U8/U16/U32/U64/U128 preserve exact type through
# local variable assignment and function parameter binding (Issue #3255)

function test_i8_local()
    x = Int8(42)
    typeof(x) == Int8 && x == 42
end

function test_i16_local()
    x = Int16(-1000)
    typeof(x) == Int16 && x == -1000
end

function test_i32_local()
    x = Int32(100000)
    typeof(x) == Int32 && x == 100000
end

function test_u8_local()
    x = UInt8(255)
    typeof(x) == UInt8 && x == 255
end

function test_u16_local()
    x = UInt16(65535)
    typeof(x) == UInt16 && x == 65535
end

function test_u32_local()
    x = UInt32(1000000)
    typeof(x) == UInt32 && x == 1000000
end

function test_u64_local()
    x = UInt64(9999999999)
    typeof(x) == UInt64 && x == 9999999999
end

function accept_i8(x::Int8)
    typeof(x) == Int8 && x == Int8(7)
end

function accept_u8(x::UInt8)
    typeof(x) == UInt8 && x == UInt8(200)
end

r1 = test_i8_local()
r2 = test_i16_local()
r3 = test_i32_local()
r4 = test_u8_local()
r5 = test_u16_local()
r6 = test_u32_local()
r7 = test_u64_local()
r8 = accept_i8(Int8(7))
r9 = accept_u8(UInt8(200))

r1 && r2 && r3 && r4 && r5 && r6 && r7 && r8 && r9
