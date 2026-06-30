using Test

@testset "Bool scalar multiplication preserves Bool (#4624)" begin
    @test typeof(true * false) == Bool
    @test true * false == false

    acc = true
    acc = acc * false
    @test typeof(acc) == Bool
    @test acc == false
end

@testset "reduce/fold plus typed multiplicative reductions (#4019, #4623)" begin
    bool_values = reduce(*, Bool[true, false, true])
    @test typeof(bool_values) == Bool
    @test bool_values == false

    bool_empty = reduce(*, Bool[])
    @test typeof(bool_empty) == Bool
    @test bool_empty == true

    bool_init = foldl(*, Bool[true, false]; init=true)
    @test typeof(bool_init) == Bool
    @test bool_init == false

    i8 = reduce(*, Int8[1, 2, 3])
    @test typeof(i8) == Int8
    @test i8 == Int8(6)

    i8_empty = reduce(*, Int8[])
    @test typeof(i8_empty) == Int8
    @test i8_empty == Int8(1)

    i8_init = foldl(*, Int8[1, 2, 3]; init=Int8(4))
    @test typeof(i8_init) == Int8
    @test i8_init == Int8(24)

    u8 = foldr(*, UInt8[1, 2, 3])
    @test typeof(u8) == UInt8
    @test u8 == UInt8(6)

    u8_empty = foldr(*, UInt8[])
    @test typeof(u8_empty) == UInt8
    @test u8_empty == UInt8(1)

    u8_init = reduce(*, UInt8[1, 2, 3]; init=UInt8(4))
    @test typeof(u8_init) == UInt8
    @test u8_init == UInt8(24)

    f32 = foldl(*, Float32[1, 2, 3])
    @test typeof(f32) == Float32
    @test f32 == Float32(6)

    f32_empty = foldr(*, Float32[])
    @test typeof(f32_empty) == Float32
    @test f32_empty == Float32(1)
end

@testset "mapfold identity typed multiplicative reductions (#4019, #4623)" begin
    bool_values = mapreduce(identity, *, Bool[true, false, true])
    @test typeof(bool_values) == Bool
    @test bool_values == false

    bool_empty = mapreduce(identity, *, Bool[])
    @test typeof(bool_empty) == Bool
    @test bool_empty == true

    i16 = mapfoldl(identity, *, Int16[1, 2, 3])
    @test typeof(i16) == Int16
    @test i16 == Int16(6)

    i16_empty = mapfoldl(identity, *, Int16[])
    @test typeof(i16_empty) == Int16
    @test i16_empty == Int16(1)

    i16_init = mapreduce(identity, *, Int16[1, 2, 3]; init=Int16(4))
    @test typeof(i16_init) == Int16
    @test i16_init == Int16(24)

    u16 = mapfoldr(identity, *, UInt16[1, 2, 3])
    @test typeof(u16) == UInt16
    @test u16 == UInt16(6)

    u16_empty = mapfoldr(identity, *, UInt16[])
    @test typeof(u16_empty) == UInt16
    @test u16_empty == UInt16(1)

    u16_init = mapfoldr(identity, *, UInt16[1, 2, 3]; init=UInt16(4))
    @test typeof(u16_init) == UInt16
    @test u16_init == UInt16(24)

    f32 = mapreduce(identity, *, Float32[1, 2, 3])
    @test typeof(f32) == Float32
    @test f32 == Float32(6)

    f32_empty = mapfoldr(identity, *, Float32[])
    @test typeof(f32_empty) == Float32
    @test f32_empty == Float32(1)

    i64_empty = mapreduce(identity, *, Int64[])
    @test typeof(i64_empty) == Int64
    @test i64_empty == 1

    f64_empty = mapfoldl(identity, *, Float64[])
    @test typeof(f64_empty) == Float64
    @test f64_empty == 1.0
end

true
