using Test

# Issue #5693: reverse(v, start[, stop]) reverses only the subrange [start, stop]
# of a vector (stop defaults to the last index), returning a copy.

@testset "reverse(v, start, stop) reverses a subrange (Issue #5693)" begin
    @test reverse([1, 2, 3, 4], 2, 3) == [1, 3, 2, 4]
    @test reverse([1, 2, 3, 4, 5], 2, 4) == [1, 4, 3, 2, 5]
    @test reverse([10, 20, 30], 1, 3) == [30, 20, 10]
    @test reverse(["a", "b", "c", "d"], 2, 3) == ["a", "c", "b", "d"]
    @test reverse([1, 2, 3], 2, 2) == [1, 2, 3]   # single element: no change

    # 2-arg form: reverse from start to the end.
    @test reverse([1, 2, 3, 4], 2) == [1, 4, 3, 2]
    @test reverse([1, 2, 3, 4], 1) == [4, 3, 2, 1]

    # Non-mutating, and type-preserving.
    v = [1, 2, 3, 4]
    @test reverse(v, 2, 3) == [1, 3, 2, 4]
    @test v == [1, 2, 3, 4]
    @test typeof(reverse([1, 2, 3, 4], 2, 3)) === Vector{Int64}

    # Whole-vector reverse is unchanged.
    @test reverse([1, 2, 3, 4]) == [4, 3, 2, 1]
end

true
