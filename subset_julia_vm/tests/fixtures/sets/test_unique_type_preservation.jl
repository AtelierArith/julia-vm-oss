using Test

# Regression test for Issue #3580:
# `unique(::Vector{Int64})` previously returned `Vector{Float64}` because
# the implementation pre-allocated `result = zeros(count)`. After Issue
# #3656 it returned `Vector{Any}` (push! to `[]`). Per the #3580 acceptance
# criteria, the result element type must match the input.
#
# Implementation: dispatch-based specialization on the common Vector{T}
# types (Int64/Float64/Bool/String/Char) seeds a typed empty `T[]`. Additional
# concrete Vector paths route through `similar(arr, 0)` so newer element types
# preserve the input element type as part of Issue #4018.

@testset "unique preserves Vector{Int64} (#3580)" begin
    x = unique([1, 2, 1])
    @test x == [1, 2]
    @test typeof(x) === Vector{Int64}

    # All-unique case
    y = unique([3, 1, 2])
    @test y == [3, 1, 2]
    @test typeof(y) === Vector{Int64}

    # All duplicates collapse to one element
    z = unique([5, 5, 5])
    @test z == [5]
    @test typeof(z) === Vector{Int64}
end

@testset "unique preserves Vector{Float64}" begin
    x = unique([1.0, 2.0, 1.0])
    @test x == [1.0, 2.0]
    @test typeof(x) === Vector{Float64}
end

@testset "unique preserves Vector{Bool}" begin
    x = unique([true, false, true])
    @test x == [true, false]
    @test typeof(x) === Vector{Bool}
end

@testset "unique preserves Vector{String}" begin
    x = unique(["apple", "banana", "apple"])
    @test x == ["apple", "banana"]
    @test typeof(x) === Vector{String}
end

@testset "unique on empty typed vector" begin
    x = unique(Int64[])
    @test length(x) == 0
    @test typeof(x) === Vector{Int64}
end

@testset "unique uses similar fallback for narrow integer vectors (#4018)" begin
    x = unique(Int8[1, 2, 1])
    @test x == Int8[1, 2]
    @test typeof(x) === Vector{Int8}
    @test eltype(x) === Int8

    y = unique(Int16[3, 3, 4])
    @test y == Int16[3, 4]
    @test typeof(y) === Vector{Int16}
    @test eltype(y) === Int16
end

@testset "unique uses similar fallback for Symbol and Any vectors (#4018)" begin
    x = unique([:a, :b, :a])
    @test x == [:a, :b]
    @test typeof(x) === Vector{Symbol}
    @test eltype(x) === Symbol

    y = unique(Any[1, 2, 1])
    @test y == Any[1, 2]
    @test typeof(y) === Vector{Any}
    @test eltype(y) === Any
end

@testset "unique on mixed Any vectors uses isequal safely (#4587)" begin
    x = unique(Any[1, "a", 1])
    @test x == Any[1, "a"]
    @test typeof(x) === Vector{Any}
    @test eltype(x) === Any

    y = unique(Any["a", 1, "a", 2])
    @test y == Any["a", 1, 2]
    @test typeof(y) === Vector{Any}
    @test eltype(y) === Any
end

@testset "unique(f, arr) preserves Vector{Int64}" begin
    # Result element type must match `arr` (not the codomain of `f`).
    x = unique(x -> x % 3, [1, 2, 3, 4, 5, 6])
    @test x == [1, 2, 3]
    @test typeof(x) === Vector{Int64}
end

@testset "unique(f, arr) preserves Vector{String}" begin
    x = unique(length, ["a", "bb", "c", "dd", "eee"])
    @test x == ["a", "bb", "eee"]
    @test typeof(x) === Vector{String}
end

@testset "unique(f, arr) uses similar fallback for narrow vectors (#4018)" begin
    x = unique(identity, Int16[1, 2, 1])
    @test x == Int16[1, 2]
    @test typeof(x) === Vector{Int16}
    @test eltype(x) === Int16
end

true
