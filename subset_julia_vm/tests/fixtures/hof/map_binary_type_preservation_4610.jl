using Test

@testset "binary map preserves representative result element types (#4019, #4610)" begin
    i8 = map(+, Int8[1, 2, 3], Int8[4, 5, 6])
    @test typeof(i8) == Vector{Int8}
    @test eltype(i8) == Int8
    @test typeof(i8[1]) == Int8
    @test i8 == Int8[5, 7, 9]

    i16 = map(*, Int16[1, 2, 3], Int16[4, 5, 6])
    @test typeof(i16) == Vector{Int16}
    @test eltype(i16) == Int16
    @test typeof(i16[1]) == Int16
    @test i16 == Int16[4, 10, 18]

    u8 = map(+, UInt8[1, 2, 3], UInt8[4, 5, 6])
    @test typeof(u8) == Vector{UInt8}
    @test eltype(u8) == UInt8
    @test typeof(u8[1]) == UInt8
    @test u8 == UInt8[5, 7, 9]

    f32 = map(*, Float32[1, 2, 3], Float32[4, 5, 6])
    @test typeof(f32) == Vector{Float32}
    @test eltype(f32) == Float32
    @test typeof(f32[1]) == Float32
    @test f32 == Float32[4, 10, 18]

    bool_sum = map(+, Bool[true, false], Bool[false, true])
    @test typeof(bool_sum) == Vector{Int64}
    @test eltype(bool_sum) == Int64
    @test typeof(bool_sum[1]) == Int64
    @test bool_sum == [1, 1]

    bool_prod = map(*, Bool[true, false], Bool[false, true])
    @test typeof(bool_prod) == Vector{Bool}
    @test eltype(bool_prod) == Bool
    @test typeof(bool_prod[1]) == Bool
    @test bool_prod == Bool[false, false]

    empty_i8_div = map(/, Int8[], Int8[])
    @test typeof(empty_i8_div) == Vector{Float64}
    @test eltype(empty_i8_div) == Float64
    @test length(empty_i8_div) == 0

    empty_bool_div = map(/, Bool[], Bool[])
    @test typeof(empty_bool_div) == Vector{Float64}
    @test eltype(empty_bool_div) == Float64
    @test length(empty_bool_div) == 0

    i8_div = map(/, Int8[1, 2], Int8[2, 4])
    @test typeof(i8_div) == Vector{Float64}
    @test eltype(i8_div) == Float64
    @test i8_div == [0.5, 0.5]

    words = map((x, y) -> x * y, String["a", "b"], String["c", "d"])
    @test typeof(words) == Vector{String}
    @test eltype(words) == String
    @test typeof(words[1]) == String
    @test words == String["ac", "bd"]
end

true
