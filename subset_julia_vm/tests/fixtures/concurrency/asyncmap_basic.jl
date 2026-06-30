# Test asyncmap: async/parallel version of map (Issue #3500)
#
# Note: SubsetJuliaVM uses a cooperative single-threaded task model
# (see src/julia/base/task.jl). Tasks scheduled via @async run immediately,
# so asyncmap returns the same results as map but exercises the Task plumbing.

using Test

@testset "asyncmap with single collection" begin
    @test asyncmap(x -> x * 2, [1, 2, 3]) == [2, 4, 6]
    @test asyncmap(x -> x + 1, [10, 20, 30]) == [11, 21, 31]
end

@testset "asyncmap with empty collection" begin
    @test asyncmap(x -> x * 2, Int[]) == Int[]
end

@testset "asyncmap with ntasks keyword" begin
    @test asyncmap(x -> x * 2, [1, 2, 3]; ntasks = 2) == [2, 4, 6]
    @test asyncmap(x -> x * 2, [1, 2, 3, 4, 5]; ntasks = 1) == [2, 4, 6, 8, 10]
    @test asyncmap(x -> x * 2, [1, 2, 3, 4, 5]; ntasks = 10) == [2, 4, 6, 8, 10]
    # ntasks=0 means "auto"; should still produce correct results
    @test asyncmap(x -> x * 2, [1, 2, 3]; ntasks = 0) == [2, 4, 6]
end

@testset "asyncmap with multiple collections" begin
    @test asyncmap(+, [1, 2, 3], [10, 20, 30]) == [11, 22, 33]
    @test asyncmap((x, y) -> x * y, [1, 2, 3], [4, 5, 6]) == [4, 10, 18]
end

_odd_or_float(x) = isodd(x) ? x : Float64(x)

@testset "asyncmap with heterogeneous result" begin
    # mixed-type results — element type widens to Any/Real
    r = asyncmap(_odd_or_float, [1, 2, 3, 4])
    @test r[1] === 1
    @test r[2] === 2.0
    @test r[3] === 3
    @test r[4] === 4.0
end

@testset "asyncmap with String collection" begin
    @test asyncmap(uppercase, ["abc", "de", "f"]) == ["ABC", "DE", "F"]
end

@testset "asyncmap over range" begin
    @test asyncmap(x -> x * x, 1:4) == [1, 4, 9, 16]
end

@testset "asyncmap with batch_size" begin
    # batch_size: f receives a Vector of argument tuples and must return a vector
    batch_f = batch -> [t[1] * 10 for t in batch]
    @test asyncmap(batch_f, 1:5; ntasks = 2, batch_size = 2) == [10, 20, 30, 40, 50]
end

@testset "asyncmap argument validation" begin
    # ntasks must be a non-negative integer
    @test_throws ArgumentError asyncmap(identity, [1, 2, 3]; ntasks = -1)
    # batch_size must be a positive integer if specified
    @test_throws ArgumentError asyncmap(identity, [1, 2, 3]; batch_size = 0)
    @test_throws ArgumentError asyncmap(identity, [1, 2, 3]; batch_size = -3)
end

true
