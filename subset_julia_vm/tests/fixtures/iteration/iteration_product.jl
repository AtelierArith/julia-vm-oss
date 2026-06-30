# Test product iterator - Cartesian product of iterables

using Test
using Iterators

@testset "product - Cartesian product of iterables" begin

    p = product([1, 2], [10, 20])

    next = iterate(p)
    @test next !== nothing
    pair1 = next[1]
    @test pair1[1] == 1
    @test pair1[2] == 10

    next = iterate(p, next[2])
    @test next !== nothing
    pair2 = next[1]
    @test pair2[1] == 2
    @test pair2[2] == 10

    next = iterate(p, next[2])
    @test next !== nothing
    pair3 = next[1]
    @test pair3[1] == 1
    @test pair3[2] == 20

    next = iterate(p, next[2])
    @test next !== nothing
    pair4 = next[1]
    @test pair4[1] == 2
    @test pair4[2] == 20

    next = iterate(p, next[2])
    @test next === nothing
    @test length(p) == 4
    @test size(p) == (2, 2)
    @test IteratorSize(p) isa HasShape{2}
    @test IteratorEltype(p) isa HasEltype
    @test eltype(p) == Tuple{Int64, Int64}
end

@testset "product - vararg ProductIterator follows upstream order (Issue #4149)" begin
    p = product([1, 2], [10, 20], [100, 200])
    @test length(p) == 8
    @test size(p) == (2, 2, 2)
    @test axes(p) == (1:2, 1:2, 1:2)
    @test ndims(p) == 3
    @test IteratorSize(p) isa HasShape{3}
    @test IteratorEltype(p) isa HasEltype
    @test eltype(p) == Tuple{Int64, Int64, Int64}

    next = iterate(p)
    @test next !== nothing
    @test next[1] == (1, 10, 100)

    next = iterate(p, next[2])
    @test next !== nothing
    @test next[1] == (2, 10, 100)

    next = iterate(p, next[2])
    @test next !== nothing
    @test next[1] == (1, 20, 100)

    values = collect(p)
    @test size(values) == (2, 2, 2)
    @test values[1, 1, 1] == (1, 10, 100)
    @test values[2, 1, 1] == (2, 10, 100)
    @test values[1, 2, 1] == (1, 20, 100)
    @test values[2, 2, 2] == (2, 20, 200)
end

@testset "product - vararg splat dispatch (Issue #4149)" begin
    iters = ([1, 2], [10, 20], [100, 200])
    p = product(iters...)
    @test collect(p)[2, 2, 2] == (2, 20, 200)

    q = Iterators.product(iters...)
    @test collect(q)[1, 2, 1] == (1, 20, 100)
end

@testset "product - singleton and empty varargs (Issue #4149)" begin
    p1 = product([1, 2])
    @test length(p1) == 2
    @test size(p1) == (2,)
    @test eltype(p1) == Tuple{Int64}
    values1 = collect(p1)
    @test typeof(values1) === Vector{Tuple{Int64}}
    @test eltype(values1) === Tuple{Int64}
    @test values1 == [(1,), (2,)]

    p0 = product()
    @test length(p0) == 1
    @test size(p0) == ()
    @test eltype(p0) == Tuple{}
    next = iterate(p0)
    @test next !== nothing
    @test next[1] == ()
    @test iterate(p0, next[2]) === nothing

    values0 = collect(p0)
    @test typeof(values0) === Array{Tuple{}, 0}
    @test string(eltype(values0)) == "Tuple{}"
    @test size(values0) == ()
    @test length(values0) == 1
    @test ndims(values0) == 0
    @test getindex(values0) == ()
    @test values0[1] == ()
end

@testset "product - generic ProductIterator arity (Issue #4150)" begin
    p5 = product([1, 2], [3, 4], [5, 6], [7, 8], [9, 10])
    @test length(p5) == 32
    @test size(p5) == (2, 2, 2, 2, 2)
    @test axes(p5) == (1:2, 1:2, 1:2, 1:2, 1:2)
    @test ndims(p5) == 5
    @test IteratorSize(p5) isa HasShape{5}
    @test IteratorEltype(p5) isa HasEltype
    @test eltype(p5) == Tuple{Int64, Int64, Int64, Int64, Int64}

    values = collect(p5)
    @test typeof(values) === Array{Tuple{Int64, Int64, Int64, Int64, Int64}, 5}
    @test eltype(values) === Tuple{Int64, Int64, Int64, Int64, Int64}
    @test size(values) == (2, 2, 2, 2, 2)
    @test values[1, 1, 1, 1, 1] == (1, 3, 5, 7, 9)
    @test values[2, 2, 2, 2, 2] == (2, 4, 6, 8, 10)

    iters = ([1, 2], [3, 4], [5, 6], [7, 8], [9, 10], [11, 12])
    p6 = product(iters...)
    @test length(p6) == 64
    @test size(p6) == (2, 2, 2, 2, 2, 2)
    @test IteratorSize(p6) isa HasShape{6}
    @test eltype(p6) == Tuple{Int64, Int64, Int64, Int64, Int64, Int64}

    next = iterate(p6)
    @test next !== nothing
    @test next[1] == (1, 3, 5, 7, 9, 11)
    next = iterate(p6, next[2])
    @test next !== nothing
    @test next[1] == (2, 3, 5, 7, 9, 11)
end

true  # Test passed
