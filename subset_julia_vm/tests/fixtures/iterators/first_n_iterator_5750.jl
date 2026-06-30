using Test

# Issue #5750: first(itr, n) must work for non-indexable iterators (generators,
# tuples, Iterators.cycle, ...), returning the first n elements as a Vector.
# Previously it used length + indexing and failed on those.

@testset "first(itr, n) for iterators (Issue #5750)" begin
    # Generators
    @test first((x^2 for x in 1:10), 3) == [1, 4, 9]
    @test first((x^2 for x in 1:10), 3) isa Vector{Int64}
    @test first((x for x in 1:5), 10) == [1, 2, 3, 4, 5]   # n larger than length

    # Tuples (upstream returns a Vector)
    @test first((1, 2, 3, 4), 2) == [1, 2]
    @test first((10, 20, 30), 3) == [10, 20, 30]

    # Iterators.cycle / take
    @test first(Iterators.cycle([1, 2, 3]), 4) == [1, 2, 3, 1]
    @test first(Iterators.take(1:100, 50), 3) == [1, 2, 3]

    # Indexable collections keep their type-preserving behavior (regression guard)
    @test first([10, 20, 30], 2) == [10, 20]
    @test first("hello", 3) == "hel"
    @test first("hello", 3) isa AbstractString
    @test first([1, 2, 3], 0) == Int64[]   # n == 0 on an indexable collection
end

true
