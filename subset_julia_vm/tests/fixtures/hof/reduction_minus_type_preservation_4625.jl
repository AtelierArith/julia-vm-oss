using Test

@testset "left subtractive reductions preserve small integer result types (#4019, #4625)" begin
    i8 = reduce(-, Int8[10, 3, 2])
    @test typeof(i8) == Int8
    @test i8 == Int8(5)

    i8_init = reduce(-, Int8[10, 3, 2]; init=Int8(0))
    @test typeof(i8_init) == Int8
    @test i8_init == Int8(-15)

    u8 = foldl(-, UInt8[10, 3, 2])
    @test typeof(u8) == UInt8
    @test u8 == UInt8(5)

    u8_init = foldl(-, UInt8[10, 3, 2]; init=UInt8(0))
    @test typeof(u8_init) == UInt8
    @test u8_init == UInt8(241)

    f32 = reduce(-, Float32[10, 3, 2])
    @test typeof(f32) == Float32
    @test f32 == Float32(5)
end

@testset "right subtractive reductions preserve small integer result types (#4019, #4625)" begin
    i16 = foldr(-, Int16[10, 3, 2])
    @test typeof(i16) == Int16
    @test i16 == Int16(9)

    i16_init = foldr(-, Int16[10, 3, 2]; init=Int16(0))
    @test typeof(i16_init) == Int16
    @test i16_init == Int16(9)

    u16 = foldr(-, UInt16[10, 3, 2])
    @test typeof(u16) == UInt16
    @test u16 == UInt16(9)

    u16_init = foldr(-, UInt16[10, 3, 2]; init=UInt16(0))
    @test typeof(u16_init) == UInt16
    @test u16_init == UInt16(9)

    f32 = foldr(-, Float32[10, 3, 2])
    @test typeof(f32) == Float32
    @test f32 == Float32(9)
end

@testset "mapfold subtractive reductions preserve small integer result types (#4019, #4625)" begin
    i32 = mapreduce(identity, -, Int32[10, 3, 2])
    @test typeof(i32) == Int32
    @test i32 == Int32(5)

    i32_init = mapfoldl(identity, -, Int32[10, 3, 2]; init=Int32(0))
    @test typeof(i32_init) == Int32
    @test i32_init == Int32(-15)

    u32 = mapreduce(identity, -, UInt32[10, 3, 2])
    @test typeof(u32) == UInt32
    @test u32 == UInt32(5)

    u8_init = mapreduce(identity, -, UInt8[10, 3, 2]; init=UInt8(0))
    @test typeof(u8_init) == UInt8
    @test u8_init == UInt8(241)

    i8_right = mapfoldr(identity, -, Int8[10, 3, 2])
    @test typeof(i8_right) == Int8
    @test i8_right == Int8(9)

    u8_right = mapfoldr(identity, -, UInt8[10, 3, 2]; init=UInt8(0))
    @test typeof(u8_right) == UInt8
    @test u8_right == UInt8(9)
end

true
