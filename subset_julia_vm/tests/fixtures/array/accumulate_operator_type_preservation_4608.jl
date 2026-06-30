using Test

@testset "accumulate operator paths preserve typed allocation (#4018, #4608)" begin
    i8 = zeros(Int8, 3)
    i8[1] = 1
    i8[2] = 2
    i8[3] = 3
    ri8 = accumulate(+, i8)
    @test typeof(ri8) == Vector{Int8}
    @test eltype(ri8) == Int8
    @test typeof(ri8[1]) == Int8
    @test ri8 == Int8[1, 3, 6]

    i16 = zeros(Int16, 3)
    i16[1] = 1
    i16[2] = 2
    i16[3] = 3
    ri16 = accumulate(*, i16)
    @test typeof(ri16) == Vector{Int16}
    @test eltype(ri16) == Int16
    @test typeof(ri16[1]) == Int16
    @test ri16 == Int16[1, 2, 6]

    i32 = zeros(Int32, 3)
    i32[1] = 1
    i32[2] = 2
    i32[3] = 3
    ri32 = accumulate(+, i32)
    @test typeof(ri32) == Vector{Int32}
    @test eltype(ri32) == Int32
    @test typeof(ri32[1]) == Int32
    @test ri32 == Int32[1, 3, 6]

    u8 = zeros(UInt8, 3)
    u8[1] = 1
    u8[2] = 2
    u8[3] = 3
    ru8 = accumulate(+, u8)
    @test typeof(ru8) == Vector{UInt8}
    @test eltype(ru8) == UInt8
    @test typeof(ru8[1]) == UInt8
    @test ru8 == UInt8[1, 3, 6]

    u16 = zeros(UInt16, 3)
    u16[1] = 1
    u16[2] = 2
    u16[3] = 3
    ru16 = accumulate(*, u16)
    @test typeof(ru16) == Vector{UInt16}
    @test eltype(ru16) == UInt16
    @test typeof(ru16[1]) == UInt16
    @test ru16 == UInt16[1, 2, 6]

    u32 = zeros(UInt32, 3)
    u32[1] = 1
    u32[2] = 2
    u32[3] = 3
    ru32 = accumulate(+, u32)
    @test typeof(ru32) == Vector{UInt32}
    @test eltype(ru32) == UInt32
    @test typeof(ru32[1]) == UInt32
    @test ru32 == UInt32[1, 3, 6]

    u64 = zeros(UInt64, 3)
    u64[1] = 1
    u64[2] = 2
    u64[3] = 3
    ru64 = accumulate(*, u64)
    @test typeof(ru64) == Vector{UInt64}
    @test eltype(ru64) == UInt64
    @test typeof(ru64[1]) == UInt64
    @test ru64 == UInt64[1, 2, 6]

    f32 = zeros(Float32, 3)
    f32[1] = 1
    f32[2] = 2
    f32[3] = 3
    rf32 = accumulate(+, f32)
    @test typeof(rf32) == Vector{Float32}
    @test eltype(rf32) == Float32
    @test typeof(rf32[1]) == Float32
    @test rf32 == Float32[1, 3, 6]

    b = Bool[true, false, true]
    rb_sum = accumulate(+, b)
    @test typeof(rb_sum) == Vector{Int64}
    @test eltype(rb_sum) == Int64
    @test typeof(rb_sum[1]) == Int64
    @test rb_sum == [1, 1, 2]

    rb_prod = accumulate(*, b)
    @test typeof(rb_prod) == Vector{Bool}
    @test eltype(rb_prod) == Bool
    @test typeof(rb_prod[1]) == Bool
    @test rb_prod == Bool[true, false, false]

    generic_i8 = accumulate((x, y) -> x + y, i8)
    @test typeof(generic_i8) == Vector{Int8}
    @test eltype(generic_i8) == Int8
    @test typeof(generic_i8[1]) == Int8
    @test generic_i8 == Int8[1, 3, 6]

    generic_i8_to_f32 = accumulate((x, y) -> Float32(x) + Float32(y), i8)
    @test typeof(generic_i8_to_f32) == Vector{Float32}
    @test eltype(generic_i8_to_f32) == Float32
    @test typeof(generic_i8_to_f32[1]) == Float32
    @test generic_i8_to_f32 == Float32[1, 3, 6]

    generic_i8_pairs = accumulate((x, y) -> x => y, i8)
    @test typeof(generic_i8_pairs) == Vector{Any}
    @test eltype(generic_i8_pairs) == Any
    @test generic_i8_pairs[2].first == Int8(1)
    @test generic_i8_pairs[2].second == Int8(2)
    @test generic_i8_pairs[3].second == Int8(3)

    div_one_i8 = accumulate(/, Int8[2])
    @test typeof(div_one_i8) == Vector{Float64}
    @test eltype(div_one_i8) == Float64
    @test typeof(div_one_i8[1]) == Float64
    @test div_one_i8 == Float64[2]

    generic_i8_to_f32_one = accumulate((x, y) -> Float32(x) + Float32(y), Int8[1])
    @test typeof(generic_i8_to_f32_one) == Vector{Float32}
    @test eltype(generic_i8_to_f32_one) == Float32
    @test typeof(generic_i8_to_f32_one[1]) == Float32
    @test generic_i8_to_f32_one == Float32[1]

    generic_i8_pair_one = accumulate((x, y) -> x => y, Int8[1])
    @test typeof(generic_i8_pair_one) == Vector{Any}
    @test eltype(generic_i8_pair_one) == Any
    @test typeof(generic_i8_pair_one[1]) == Int8
    @test generic_i8_pair_one[1] == Int8(1)

    div_empty_i8 = accumulate(/, Int8[])
    @test typeof(div_empty_i8) == Vector{Float64}
    @test eltype(div_empty_i8) == Float64
    @test length(div_empty_i8) == 0

    generic_i8_to_f32_empty = accumulate((x, y) -> Float32(x) + Float32(y), Int8[])
    @test typeof(generic_i8_to_f32_empty) == Vector{Float32}
    @test eltype(generic_i8_to_f32_empty) == Float32
    @test length(generic_i8_to_f32_empty) == 0

    generic_i8_pair_empty = accumulate((x, y) -> x => y, Int8[])
    @test typeof(generic_i8_pair_empty) == Vector{Any}
    @test eltype(generic_i8_pair_empty) == Any
    @test length(generic_i8_pair_empty) == 0

    words = String["a", "b", "c"]
    generic_words = accumulate((x, y) -> x * y, words)
    @test typeof(generic_words) == Vector{String}
    @test eltype(generic_words) == String
    @test typeof(generic_words[1]) == String
    @test generic_words == String["a", "ab", "abc"]

    empty_i8 = accumulate((x, y) -> x + y, Int8[])
    @test typeof(empty_i8) == Vector{Int8}
    @test eltype(empty_i8) == Int8
    @test length(empty_i8) == 0
end

true
