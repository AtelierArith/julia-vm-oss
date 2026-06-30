using Test

@testset "similar preserves String element type for literal vectors (Issue #4278)" begin
    v = ["a", "b"]

    same_len = similar(v)
    @test typeof(same_len) === Vector{String}
    @test eltype(same_len) === String
    @test length(same_len) == 2

    resized = similar(v, 3)
    @test typeof(resized) === Vector{String}
    @test eltype(resized) === String
    @test length(resized) == 3
end

@testset "repeat preserves String element type for literal vectors (Issue #4278)" begin
    v = ["a", "b"]

    r = repeat(v, 2)
    @test typeof(r) === Vector{String}
    @test r == ["a", "b", "a", "b"]

    tiled = repeat(v, 2, 2)
    @test typeof(tiled) === Matrix{String}
    @test size(tiled) == (4, 2)
    @test tiled == ["a" "a"; "b" "b"; "a" "a"; "b" "b"]
end

@testset "repeat preserves String element type for literal matrices (Issue #4278)" begin
    m = ["a" "b"; "c" "d"]
    r = repeat(m, 2, 1)

    @test typeof(r) === Matrix{String}
    @test size(r) == (4, 2)
    @test r == ["a" "b"; "c" "d"; "a" "b"; "c" "d"]
end

@testset "permutedims preserves String element type for literal arrays (Issue #4278)" begin
    v = ["a", "b"]
    row = permutedims(v)
    @test typeof(row) === Matrix{String}
    @test size(row) == (1, 2)
    @test row == ["a" "b"]

    m = ["a" "b"; "c" "d"]
    transposed = permutedims(m)
    @test typeof(transposed) === Matrix{String}
    @test size(transposed) == (2, 2)
    @test transposed == ["a" "c"; "b" "d"]

    copied = permutedims(m, (1, 2))
    @test typeof(copied) === Matrix{String}
    @test size(copied) == (2, 2)
    @test copied == m
end

true
