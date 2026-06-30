using Test

# Regression test for Issue #3587:
# `repeat(arr, n)` previously hard-coded `Vector{Float64}` output via
# `zeros(len * n)`, widening any non-Float input. Now uses push! onto an
# empty `[]` so the values are preserved (element type is Any until the
# deeper VM type-preservation infra in #3648 lands).

@testset "repeat preserves values for Int (#3587)" begin
    x = repeat([1, 2], 2)
    @test x == [1, 2, 1, 2]
    @test length(x) == 4
end

@testset "repeat preserves values for Bool" begin
    x = repeat([true, false], 2)
    @test x == [true, false, true, false]
end

@testset "repeat preserves values for String" begin
    x = repeat(["a", "b"], 3)
    @test x == ["a", "b", "a", "b", "a", "b"]
end

@testset "repeat regression for Float64" begin
    x = repeat([1.0, 2.0], 2)
    @test x == [1.0, 2.0, 1.0, 2.0]
end

@testset "repeat edge cases" begin
    @test repeat([1, 2], 0) == []
    @test repeat([1], 1) == [1]
    @test repeat(Int[], 5) == []
end

@testset "repeat(arr, m, n) preserves matrix element type (#3761)" begin
    vi = repeat([1, 2], 2, 3)
    @test typeof(vi) === Matrix{Int64}
    @test size(vi) == (4, 3)
    @test vi == [1 1 1; 2 2 2; 1 1 1; 2 2 2]

    vb = repeat([true, false], 2, 2)
    @test typeof(vb) === Matrix{Bool}
    @test size(vb) == (4, 2)

    mi = repeat([1 2; 3 4], 2, 2)
    @test typeof(mi) === Matrix{Int64}
    @test size(mi) == (4, 4)
    @test mi[1, 1] == 1
    @test mi[3, 3] == 1
    @test mi[4, 4] == 4
end

true
