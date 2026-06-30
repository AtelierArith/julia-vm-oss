using Test

@testset "empty Set{T} constructor preserves element type (#4018, #4661)" begin
    s = Set{Int8}()
    @test typeof(s) === Set{Int8}
    @test eltype(s) === Int8

    empty_collect = collect(s)
    @test typeof(empty_collect) === Vector{Int8}
    @test eltype(empty_collect) === Int8
    @test length(empty_collect) == 0

    s = push!(s, Int8(1))
    @test typeof(s) === Set{Int8}
    @test eltype(s) === Int8

    pushed_collect = collect(s)
    @test typeof(pushed_collect) === Vector{Int8}
    @test eltype(pushed_collect) === Int8
    @test pushed_collect == Int8[1]

    push!(s, Int8(2))
    mutated_collect = collect(s)
    @test typeof(mutated_collect) === Vector{Int8}
    @test eltype(mutated_collect) === Int8
    @test length(mutated_collect) == 2
    @test Int8(1) in mutated_collect
    @test Int8(2) in mutated_collect

    emptied = empty!(s)
    @test typeof(emptied) === Set{Int8}
    @test eltype(emptied) === Int8
    @test typeof(s) === Set{Int8}
    @test eltype(s) === Int8

    emptied_collect = collect(s)
    @test typeof(emptied_collect) === Vector{Int8}
    @test eltype(emptied_collect) === Int8
    @test length(emptied_collect) == 0
end

true
