using Test

@testset "generator collect preserves narrow source eltype (#4018, #4631)" begin
    tuple_i8 = collect(x for x in (Int8(1), Int8(2)))
    @test typeof(tuple_i8) === Vector{Int8}
    @test eltype(tuple_i8) === Int8
    @test tuple_i8 == Int8[1, 2]

    array_i8 = collect(x for x in Int8[1, 2])
    @test typeof(array_i8) === Vector{Int8}
    @test eltype(array_i8) === Int8
    @test array_i8 == Int8[1, 2]

    filtered_i8 = collect(x for x in Int8[1, 2, 3] if x > Int8(1))
    @test typeof(filtered_i8) === Vector{Int8}
    @test eltype(filtered_i8) === Int8
    @test filtered_i8 == Int8[2, 3]

    array_f32 = collect(x for x in Float32[1, 2])
    @test typeof(array_f32) === Vector{Float32}
    @test eltype(array_f32) === Float32
    @test array_f32 == Float32[1, 2]

    mixed_signed = collect(x for x in (Int8(1), Int16(2)))
    @test eltype(mixed_signed) === Signed
    @test mixed_signed[1] == Int8(1)
    @test mixed_signed[2] == Int16(2)
end

true
