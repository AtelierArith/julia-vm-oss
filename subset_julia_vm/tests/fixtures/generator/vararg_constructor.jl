using Test

add_pair(x, y) = x + y
add_triple(x, y, z) = x + y + z
runtime_collect(x) = collect(x)

@testset "Base.Generator vararg constructor (Issues #4106/#4108)" begin
    pair = Base.Generator(add_pair, [1, 2, 3], [10, 20])
    pair_values = collect(pair)
    @test typeof(pair_values) === Vector{Int64}
    @test eltype(pair_values) === Int64
    @test pair_values == [11, 22]

    runtime_pair_values = runtime_collect(Base.Generator(add_pair, [1, 2, 3], [10, 20]))
    @test typeof(runtime_pair_values) === Vector{Int64}
    @test eltype(runtime_pair_values) === Int64
    @test runtime_pair_values == [11, 22]

    pair_short_first = collect(Base.Generator(add_pair, [1], [10, 20, 30]))
    @test pair_short_first == [11]
    @test length(pair_short_first) == 1

    triple = Base.Generator(add_triple, [1, 2, 3], [10, 20], [100, 200, 300, 400])
    triple_values = collect(triple)
    @test typeof(triple_values) === Vector{Int64}
    @test eltype(triple_values) === Int64
    @test triple_values == [111, 222]

    runtime_triple_values = runtime_collect(Base.Generator(add_triple, [1, 2, 3], [10, 20], [100, 200, 300, 400]))
    @test typeof(runtime_triple_values) === Vector{Int64}
    @test eltype(runtime_triple_values) === Int64
    @test runtime_triple_values == [111, 222]

    triple_short_middle = collect(Base.Generator(add_triple, [1, 2], Int64[], [100, 200]))
    @test typeof(triple_short_middle) === Vector{Int64}
    @test eltype(triple_short_middle) === Int64
    @test length(triple_short_middle) == 0

    pair_empty_first = collect(Base.Generator(add_pair, Int64[], [10, 20]))
    @test typeof(pair_empty_first) === Vector{Int64}
    @test eltype(pair_empty_first) === Int64
    @test pair_empty_first == Int64[]

    triple_empty_last = collect(Base.Generator(add_triple, [1, 2], [10, 20], Int64[]))
    @test typeof(triple_empty_last) === Vector{Int64}
    @test eltype(triple_empty_last) === Int64
    @test triple_empty_last == Int64[]
end

true
