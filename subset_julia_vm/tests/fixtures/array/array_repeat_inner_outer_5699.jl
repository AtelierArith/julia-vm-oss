using Test

# Issue #5699: repeat(v; inner=k, outer=m) — repeat each element `inner` times,
# then the whole result `outer` times. Only the positional repeat(v, n) existed.

@testset "repeat(v; inner, outer) (Issue #5699)" begin
    @test repeat([1, 2, 3], inner=2) == [1, 1, 2, 2, 3, 3]
    @test repeat([1, 2, 3], outer=2) == [1, 2, 3, 1, 2, 3]
    @test repeat([1, 2], inner=2, outer=2) == [1, 1, 2, 2, 1, 1, 2, 2]
    @test repeat([1, 2], inner=3) == [1, 1, 1, 2, 2, 2]
    @test repeat(["a", "b"], inner=2) == ["a", "a", "b", "b"]
    @test repeat([1, 2]) == [1, 2]                  # no kwargs: copy
    @test typeof(repeat([1, 2, 3], inner=2)) === Vector{Int64}

    # Positional repeat(v, n) and matrix repeat are unchanged.
    @test repeat([1, 2], 3) == [1, 2, 1, 2, 1, 2]
    @test repeat([1 2; 3 4], 2, 1) == [1 2; 3 4; 1 2; 3 4]
end

true
