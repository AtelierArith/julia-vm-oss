using Test

@testset "mapreduce identity plus typed arrays (#4019, #4619)" begin
    bool_values = mapreduce(identity, +, Bool[true, false, true])
    @test typeof(bool_values) == Int64
    @test bool_values == 2

    bool_empty = mapreduce(identity, +, Bool[])
    @test typeof(bool_empty) == Int64
    @test bool_empty == 0

    bool_empty_init = mapreduce(identity, +, Bool[]; init=false)
    @test typeof(bool_empty_init) == Bool
    @test bool_empty_init == false

    bool_init = mapreduce(identity, +, Bool[true, false]; init=false)
    @test typeof(bool_init) == Int64
    @test bool_init == 1

    i8 = mapreduce(identity, +, Int8[1, 2, 3])
    @test typeof(i8) == Int8
    @test i8 == Int8(6)

    i8_empty = mapreduce(identity, +, Int8[])
    @test typeof(i8_empty) == Int8
    @test i8_empty == Int8(0)

    i8_init = mapreduce(identity, +, Int8[1, 2, 3]; init=Int8(4))
    @test typeof(i8_init) == Int8
    @test i8_init == Int8(10)

    u8 = mapreduce(identity, +, UInt8[1, 2, 3])
    @test typeof(u8) == UInt8
    @test u8 == UInt8(6)

    u8_empty = mapreduce(identity, +, UInt8[])
    @test typeof(u8_empty) == UInt8
    @test u8_empty == UInt8(0)

    u8_init = mapreduce(identity, +, UInt8[1, 2, 3]; init=UInt8(4))
    @test typeof(u8_init) == UInt8
    @test u8_init == UInt8(10)

    f32 = mapreduce(identity, +, Float32[1, 2, 3])
    @test typeof(f32) == Float32
    @test f32 == Float32(6)

    f32_empty = mapreduce(identity, +, Float32[])
    @test typeof(f32_empty) == Float32
    @test f32_empty == Float32(0)

    f32_init = mapreduce(identity, +, Float32[1, 2, 3]; init=Float32(4))
    @test typeof(f32_init) == Float32
    @test f32_init == Float32(10)
end

@testset "mapfoldl identity plus typed arrays (#4019, #4619)" begin
    i16 = mapfoldl(identity, +, Int16[1, 2, 3])
    @test typeof(i16) == Int16
    @test i16 == Int16(6)

    i16_empty = mapfoldl(identity, +, Int16[])
    @test typeof(i16_empty) == Int16
    @test i16_empty == Int16(0)

    u16 = mapfoldl(identity, +, UInt16[1, 2, 3])
    @test typeof(u16) == UInt16
    @test u16 == UInt16(6)

    u16_empty = mapfoldl(identity, +, UInt16[])
    @test typeof(u16_empty) == UInt16
    @test u16_empty == UInt16(0)

    i64_empty = mapfoldl(identity, +, Int64[])
    @test typeof(i64_empty) == Int64
    @test i64_empty == 0

    f64_empty = mapfoldl(identity, +, Float64[])
    @test typeof(f64_empty) == Float64
    @test f64_empty == 0.0
end

true
