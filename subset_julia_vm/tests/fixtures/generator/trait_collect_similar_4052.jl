using Test

function generator_trait_double_4052(x)
    return x * 2
end

function generator_trait_tofloat_4052(x)
    return x + 0.5
end

function generator_trait_not_4018(x)
    return !x
end

@testset "VM-native Base.Generator traits and collect_similar (Issue #4052)" begin
    gv = Base.Generator(generator_trait_double_4052, [1, 2, 3])
    @test typeof(Base.IteratorSize(gv)) === typeof(Base.HasShape{1}())
    @test typeof(Base.IteratorEltype(gv)) === typeof(Base.EltypeUnknown())

    gm = Base.Generator(generator_trait_double_4052, [1 2; 3 4])
    @test typeof(Base.IteratorSize(gm)) === typeof(Base.HasShape{2}())
    @test typeof(Base.IteratorEltype(gm)) === typeof(Base.EltypeUnknown())

    shaped = Base.collect_similar([0.0], gm)
    @test typeof(shaped) === Matrix{Int64}
    @test eltype(shaped) === Int64
    @test size(shaped) == (2, 2)
    @test shaped == [2 4; 6 8]

    empty_float = Base.Generator(generator_trait_tofloat_4052, Int64[])
    collected_empty_float = Base.collect_similar([0.0], empty_float)
    @test typeof(collected_empty_float) === Vector{Float64}
    @test eltype(collected_empty_float) === Float64
    @test length(collected_empty_float) == 0

    empty_type = Base.Generator(Float64, Int64[])
    collected_empty_type = Base.collect_similar([0.0], empty_type)
    @test typeof(collected_empty_type) === Vector{Float64}
    @test eltype(collected_empty_type) === Float64
    @test length(collected_empty_type) == 0

    inline_shaped = Base.collect_similar([0.0], Base.Generator(generator_trait_double_4052, [1 2; 3 4]))
    @test typeof(inline_shaped) === Matrix{Int64}
    @test size(inline_shaped) == (2, 2)
    @test inline_shaped == [2 4; 6 8]

    bool_shaped = Base.collect_similar([false], Base.Generator(generator_trait_not_4018, [true false; false true]))
    @test typeof(bool_shaped) === Matrix{Bool}
    @test eltype(bool_shaped) === Bool
    @test size(bool_shaped) == (2, 2)
    @test bool_shaped == [false true; true false]

    char_shaped = Base.collect_similar(['a'], Base.Generator(identity, ['x' 'y'; 'z' 'w']))
    @test typeof(char_shaped) === Matrix{Char}
    @test eltype(char_shaped) === Char
    @test size(char_shaped) == (2, 2)
    @test char_shaped == ['x' 'y'; 'z' 'w']

    symbol_shaped = Base.collect_similar([:a], Base.Generator(identity, [:x :y; :z :w]))
    @test typeof(symbol_shaped) === Matrix{Symbol}
    @test eltype(symbol_shaped) === Symbol
    @test size(symbol_shaped) == (2, 2)
    @test symbol_shaped == [:x :y; :z :w]

    string_shaped = Base.collect_similar([""], Base.Generator(identity, ["aa" "bb"; "cc" "dd"]))
    @test typeof(string_shaped) === Matrix{String}
    @test eltype(string_shaped) === String
    @test size(string_shaped) == (2, 2)
    @test string_shaped == ["aa" "bb"; "cc" "dd"]

    float32_source = fill(Float32(1.5), (2, 2))
    float32_shaped = Base.collect_similar(Float32[], Base.Generator(identity, float32_source))
    @test typeof(float32_shaped) === Matrix{Float32}
    @test eltype(float32_shaped) === Float32
    @test size(float32_shaped) == (2, 2)
    @test float32_shaped[1, 1] === Float32(1.5)

    complex_shaped = Base.collect_similar([0.0 + 0.0im], Base.Generator(identity, [1.0 + 2.0im 3.0 + 4.0im; 5.0 + 6.0im 7.0 + 8.0im]))
    @test typeof(complex_shaped) === Matrix{Complex{Float64}}
    @test eltype(complex_shaped) === Complex{Float64}
    @test size(complex_shaped) == (2, 2)
    @test complex_shaped == [1.0 + 2.0im 3.0 + 4.0im; 5.0 + 6.0im 7.0 + 8.0im]

    cube_shaped = Base.collect_similar([0], Base.Generator(generator_trait_double_4052, fill(1, (2, 2, 2))))
    @test typeof(cube_shaped) === Array{Int64, 3}
    @test eltype(cube_shaped) === Int64
    @test size(cube_shaped) == (2, 2, 2)
    @test cube_shaped[2, 2, 2] == 2

    float32_cube = fill(Float32(1.5), (2, 2, 2))
    float32_cube_shaped = Base.collect_similar(Float32[], Base.Generator(identity, float32_cube))
    @test typeof(float32_cube_shaped) === Array{Float32, 3}
    @test eltype(float32_cube_shaped) === Float32
    @test size(float32_cube_shaped) == (2, 2, 2)
    @test float32_cube_shaped[2, 2, 2] === Float32(1.5)

    any_shaped = Base.collect_similar(Any[], Base.Generator(identity, [1 "x"; 2.0 true]))
    @test typeof(any_shaped) === Matrix{Any}
    @test eltype(any_shaped) === Any
    @test size(any_shaped) == (2, 2)
    @test any_shaped == [1 "x"; 2.0 true]

    inline_empty = Base.collect_similar([0.0], Base.Generator(generator_trait_tofloat_4052, Int64[]))
    @test typeof(inline_empty) === Vector{Float64}
    @test eltype(inline_empty) === Float64
    @test length(inline_empty) == 0

    memory_values = Base.collect_similar(Memory{Float64}(undef, 0), Base.Generator(generator_trait_double_4052, [1, 2, 3]))
    @test typeof(memory_values) === Memory{Int64}
    @test eltype(memory_values) === Int64
    @test length(memory_values) == 3
    @test memory_values[1] == 2
    @test memory_values[2] == 4
    @test memory_values[3] == 6

    memory_mixed = Base.collect_similar(Memory{Float64}(undef, 0), Base.Generator(identity, (1, 2.0)))
    @test typeof(memory_mixed) === Memory{Real}
    @test eltype(memory_mixed) === Real
    @test length(memory_mixed) == 2
    @test memory_mixed[1] == 1
    @test memory_mixed[2] == 2.0

    memory_empty = Base.collect_similar(Memory{Float64}(undef, 0), Base.Generator(generator_trait_tofloat_4052, Int64[]))
    @test typeof(memory_empty) === Memory{Float64}
    @test eltype(memory_empty) === Float64
    @test length(memory_empty) == 0
end

true
