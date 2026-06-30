# similar(arr, dims...) for 2+ dimensions (Issue #3751)
# `similar(arr, n, m, ...)` returns an uninitialized array of eltype(arr)
# with the given shape. `similar(arr, T, n, m, ...)` returns an uninitialized
# array with element type T and the given shape.
# PR #3746 fixed the 1D case (similar(arr) and similar(arr, n)); the multi-dim
# arity was deferred to this issue.

using Test

@testset "similar(mat, n, m) returns 2D matrix (Issue #3751)" begin
    a = [1 2; 3 4]
    b = similar(a, 2, 3)
    @test typeof(b) == Matrix{Int64}
    @test size(b) == (2, 3)

    c = [1.0 2.0; 3.0 4.0]
    d = similar(c, 4, 5)
    @test typeof(d) == Matrix{Float64}
    @test size(d) == (4, 5)
end

@testset "similar(vec, n, m) reshapes Vector to Matrix" begin
    a = [1, 2, 3]
    b = similar(a, 2, 3)
    @test typeof(b) == Matrix{Int64}
    @test size(b) == (2, 3)

    c = [1.0, 2.0]
    d = similar(c, 3, 4)
    @test typeof(d) == Matrix{Float64}
    @test size(d) == (3, 4)
end

@testset "similar with 3+ dimensions (Issue #3751)" begin
    a = [1, 2, 3]
    b = similar(a, 2, 3, 4)
    @test typeof(b) == Array{Int64, 3}
    @test size(b) == (2, 3, 4)

    c = [1.0, 2.0]
    d = similar(c, 2, 2, 2, 2)
    @test typeof(d) == Array{Float64, 4}
    @test size(d) == (2, 2, 2, 2)
end

@testset "similar(arr, T, dims...) — typed multi-dim form" begin
    a = [1 2; 3 4]
    b = similar(a, Int, 4, 5)
    @test typeof(b) == Matrix{Int64}
    @test size(b) == (4, 5)

    c = [1, 2, 3]
    d = similar(c, Float64, 2, 3)
    @test typeof(d) == Matrix{Float64}
    @test size(d) == (2, 3)

    e = similar(c, Bool, 2, 2, 2)
    @test typeof(e) == Array{Bool, 3}
    @test size(e) == (2, 2, 2)
end

@testset "similar(arr, T) — typed same-shape form" begin
    a = [1, 2, 3]
    b = similar(a, Float64)
    @test typeof(b) == Vector{Float64}
    @test length(b) == 3

    c = [1 2; 3 4]
    d = similar(c, Bool)
    @test typeof(d) == Matrix{Bool}
    @test size(d) == (2, 2)
end

@testset "similar(arr, T, n) — typed 1D form" begin
    a = [1, 2, 3]
    b = similar(a, Float64, 5)
    @test typeof(b) == Vector{Float64}
    @test length(b) == 5
end

@testset "similar(arr, dims...) inside a function (Any-typed param)" begin
    f(arr) = similar(arr, 2, 3)
    r1 = f([1, 2, 3])
    @test typeof(r1) == Matrix{Int64}
    @test size(r1) == (2, 3)
    r2 = f([1.0, 2.0])
    @test typeof(r2) == Matrix{Float64}
    @test size(r2) == (2, 3)
end

@testset "similar with assignment writes back correctly" begin
    a = [1, 2, 3]
    b = similar(a, 2, 3)
    b[1, 1] = 10
    b[2, 3] = 99
    @test b[1, 1] == 10
    @test b[2, 3] == 99
end

true
