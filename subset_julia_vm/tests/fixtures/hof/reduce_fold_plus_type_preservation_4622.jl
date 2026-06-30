using Test

@testset "reduce plus typed arrays (#4019, #4622)" begin
    bool_values = reduce(+, Bool[true, false, true])
    @test typeof(bool_values) == Int64
    @test bool_values == 2

    bool_empty = reduce(+, Bool[])
    @test typeof(bool_empty) == Int64
    @test bool_empty == 0

    bool_empty_init = reduce(+, Bool[]; init=false)
    @test typeof(bool_empty_init) == Bool
    @test bool_empty_init == false

    i8 = reduce(+, Int8[1, 2, 3])
    @test typeof(i8) == Int8
    @test i8 == Int8(6)

    i8_empty = reduce(+, Int8[])
    @test typeof(i8_empty) == Int8
    @test i8_empty == Int8(0)

    i8_init = reduce(+, Int8[1, 2, 3]; init=Int8(4))
    @test typeof(i8_init) == Int8
    @test i8_init == Int8(10)

    u8 = reduce(+, UInt8[1, 2, 3])
    @test typeof(u8) == UInt8
    @test u8 == UInt8(6)

    u8_empty = reduce(+, UInt8[])
    @test typeof(u8_empty) == UInt8
    @test u8_empty == UInt8(0)

    u8_init = reduce(+, UInt8[1, 2, 3]; init=UInt8(4))
    @test typeof(u8_init) == UInt8
    @test u8_init == UInt8(10)

    f32 = reduce(+, Float32[1, 2, 3])
    @test typeof(f32) == Float32
    @test f32 == Float32(6)

    f32_empty = reduce(+, Float32[])
    @test typeof(f32_empty) == Float32
    @test f32_empty == Float32(0)
end

@testset "foldl plus typed arrays (#4019, #4622)" begin
    i16 = foldl(+, Int16[1, 2, 3])
    @test typeof(i16) == Int16
    @test i16 == Int16(6)

    i16_empty = foldl(+, Int16[])
    @test typeof(i16_empty) == Int16
    @test i16_empty == Int16(0)

    u16 = foldl(+, UInt16[1, 2, 3])
    @test typeof(u16) == UInt16
    @test u16 == UInt16(6)

    u16_empty = foldl(+, UInt16[])
    @test typeof(u16_empty) == UInt16
    @test u16_empty == UInt16(0)

    i64_empty = foldl(+, Int64[])
    @test typeof(i64_empty) == Int64
    @test i64_empty == 0

    f64_empty = foldl(+, Float64[])
    @test typeof(f64_empty) == Float64
    @test f64_empty == 0.0
end

@testset "foldr plus typed arrays (#4019, #4622)" begin
    bool_values = foldr(+, Bool[true, false, true])
    @test typeof(bool_values) == Int64
    @test bool_values == 2

    bool_empty = foldr(+, Bool[])
    @test typeof(bool_empty) == Int64
    @test bool_empty == 0

    i32 = foldr(+, Int32[1, 2, 3])
    @test typeof(i32) == Int32
    @test i32 == Int32(6)

    i32_empty = foldr(+, Int32[])
    @test typeof(i32_empty) == Int32
    @test i32_empty == Int32(0)

    u32 = foldr(+, UInt32[1, 2, 3])
    @test typeof(u32) == UInt32
    @test u32 == UInt32(6)

    u32_empty = foldr(+, UInt32[])
    @test typeof(u32_empty) == UInt32
    @test u32_empty == UInt32(0)

    u32_init = foldr(+, UInt32[1, 2, 3]; init=UInt32(4))
    @test typeof(u32_init) == UInt32
    @test u32_init == UInt32(10)

    f32_empty = foldr(+, Float32[])
    @test typeof(f32_empty) == Float32
    @test f32_empty == Float32(0)
end

true
