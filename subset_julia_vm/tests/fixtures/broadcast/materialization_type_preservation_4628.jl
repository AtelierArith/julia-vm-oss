using Test

@testset "broadcast materialization preserves representative result types (#4019, #4628)" begin
    f32_add = Float32[1, 2] .+ Float32[3, 4]
    @test eltype(f32_add) == Float32
    @test typeof(f32_add[1]) == Float32
    @test f32_add[1] == Float32(4)

    f32_div = Float32[2, 4] ./ Float32[2, 2]
    @test eltype(f32_div) == Float32
    @test typeof(f32_div[1]) == Float32
    @test f32_div[2] == Float32(2)

    i8_add = Int8[1, 2] .+ Int8[3, 4]
    @test eltype(i8_add) == Int8
    @test typeof(i8_add[1]) == Int8
    @test i8_add[1] == Int8(4)

    i8_scalar = Int8[1, 2] .- Int8(3)
    @test eltype(i8_scalar) == Int8
    @test typeof(i8_scalar[1]) == Int8
    @test i8_scalar[1] == Int8(-2)

    u8_mul = UInt8[2, 3] .* UInt8[3, 4]
    @test eltype(u8_mul) == UInt8
    @test typeof(u8_mul[1]) == UInt8
    @test u8_mul[1] == UInt8(6)

    strings = uppercase.(String["a", "b"])
    @test eltype(strings) == String
    @test typeof(strings[1]) == String
    @test strings[1] == "A"

    matrix = Float32[1 2; 3 4] .+ Float32(1)
    @test eltype(matrix) == Float32
    @test size(matrix) == (2, 2)
    @test typeof(matrix[1, 1]) == Float32
    @test matrix[2, 2] == Float32(5)
end

true
