using Test

@testset "vararg map preserves representative result element types (#4019, #4611)" begin
    i8 = map(+, Int8[1, 2, 3], Int8[10, 20, 30], Int8[40, 50, 60])
    @test typeof(i8) == Vector{Int8}
    @test eltype(i8) == Int8
    @test typeof(i8[1]) == Int8
    @test i8 == Int8[51, 72, 93]

    i8_overflow = map(+, Int8[1, 2], Int8[10, 20], Int8[100, 110])
    @test typeof(i8_overflow) == Vector{Int8}
    @test eltype(i8_overflow) == Int8
    @test typeof(i8_overflow[2]) == Int8
    @test i8_overflow == Int8[111, -124]

    words = map((a, b, c) -> a * b * c, String["a", "b"], String["c", "d"], String["e", "f"])
    @test typeof(words) == Vector{String}
    @test eltype(words) == String
    @test typeof(words[1]) == String
    @test words == String["ace", "bdf"]

    shortest = map((a, b, c) -> a + b + c, Int64[1, 2, 3], Int64[10], Int64[100, 200])
    @test typeof(shortest) == Vector{Int64}
    @test shortest == Int64[111]

    empty = map((a, b, c) -> a + b + c, Int8[], Int8[], Int8[])
    @test typeof(empty) == Vector{Int8}
    @test eltype(empty) == Int8
    @test length(empty) == 0

    four = map(+, Int16[1, 2], Int16[10, 20], Int16[100, 200], Int16[1000, 2000])
    @test typeof(four) == Vector{Int16}
    @test eltype(four) == Int16
    @test four == Int16[1111, 2222]
end

true
