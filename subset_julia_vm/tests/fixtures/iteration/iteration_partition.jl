# Test partition iterator - group elements into chunks of size n

using Test
using Base: IteratorEltype, IteratorSize, HasEltype, HasLength
using Base.Iterators

@testset "partition - group elements into chunks" begin

    p = partition([1, 2, 3, 4, 5, 6], 2)

    next = iterate(p)
    @test next !== nothing
    chunk1 = next[1]
    @test length(chunk1) == 2
    @test collect(chunk1) == [1, 2]

    next = iterate(p, next[2])
    @test next !== nothing
    chunk2 = next[1]
    @test length(chunk2) == 2
    @test collect(chunk2) == [3, 4]

    next = iterate(p, next[2])
    @test next !== nothing
    chunk3 = next[1]
    @test length(chunk3) == 2
    @test collect(chunk3) == [5, 6]

    next = iterate(p, next[2])
    @test next === nothing
    @test length(p) == 3
    @test IteratorSize(p) isa HasLength
    @test IteratorEltype(p) isa HasEltype
    @test eltype(p) <: SubArray{Int64}
    @test_throws ArgumentError partition([1, 2], 0)
end

@testset "partition vector collect preserves chunk view eltype (Issues #4018, #4648, #4649)" begin
    chunks = collect(partition(Int8[1, 2, 3], 2))
    @test eltype(chunks) <: SubArray{Int8}
    @test length(chunks) == 2
    @test collect(chunks[1]) == Int8[1, 2]
    @test collect(chunks[2]) == Int8[3]

    v = view(Int8[4, 5, 6], 1:2)
    @test typeof(v) <: SubArray{Int8}
    @test collect(v) == Int8[4, 5]
end

true  # Test passed
