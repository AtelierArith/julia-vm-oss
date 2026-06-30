using Test

@testset "string array literals preserve Vector{String} (Issue #4277)" begin
    xs = ["a", "b", "c"]
    @test typeof(xs) === Vector{String}
    @test eltype(xs) === String
    @test xs == ["a", "b", "c"]

    typed_xs = String["a", "b"]
    @test typeof(typed_xs) === Vector{String}
    @test eltype(typed_xs) === String
    @test typed_xs == ["a", "b"]

    ys = ['a', 'b']
    @test typeof(ys) === Vector{Char}
    @test eltype(ys) === Char

    typed_ys = Char['x', 'y']
    @test typeof(typed_ys) === Vector{Char}
    @test eltype(typed_ys) === Char
    @test typed_ys == ['x', 'y']

    narrow_ints = Int8[1, 2]
    @test typeof(narrow_ints) === Vector{Int8}
    @test eltype(narrow_ints) === Int8

    mixed_any = Any["a", 1]
    @test typeof(mixed_any) === Vector{Any}
    @test eltype(mixed_any) === Any
    @test mixed_any == Any["a", 1]

    explicit_getindex = getindex(String, "a", "b")
    @test typeof(explicit_getindex) === Vector{String}
    @test explicit_getindex == ["a", "b"]
end

true
