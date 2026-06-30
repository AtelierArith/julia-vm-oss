using Test

# Regression test for Issue #3588:
# `hcat([1, 2], [3, 4])` previously returned `Matrix{Float64}` because
# the implementation pre-allocated `result = zeros(na, 2)`. Per the
# #3588 acceptance criteria, the result element type must match the
# input for the 2-argument case.
#
# Implementation: dispatch-based specialization on the common Vector{T}
# types (Int64/Float64/Bool/String/Char) seeds a typed empty `T[]` and
# reshapes after push! accumulation so the returned `Matrix{T}` matches
# the input element type. Generic 1-argument and 4+-argument fallbacks
# return `Matrix{Any}` because typed varargs tie with the generic
# `hcat(args...)` during dispatch (no AmbiguousMethod tie-breaker for
# two varargs candidates), and pure-Julia `similar` inside a function
# is blocked by Issue #3648.

@testset "hcat preserves Matrix{Int64} (#3588)" begin
    m = hcat([1, 2], [3, 4])
    @test size(m) == (2, 2)
    @test m == [1 3; 2 4]
    @test typeof(m) === Matrix{Int64}

    # 3-argument case also preserved
    m3 = hcat([1, 2], [3, 4], [5, 6])
    @test size(m3) == (2, 3)
    @test m3 == [1 3 5; 2 4 6]
    @test typeof(m3) === Matrix{Int64}
end

@testset "hcat preserves Matrix{Bool}" begin
    m = hcat([true, false], [false, true])
    @test size(m) == (2, 2)
    @test m[1, 1] == true
    @test m[2, 2] == true
    @test m[1, 2] == false
    @test typeof(m) === Matrix{Bool}
end

@testset "hcat preserves Matrix{Float64} (regression)" begin
    m = hcat([1.0, 2.0], [3.0, 4.0])
    @test size(m) == (2, 2)
    @test m == [1.0 3.0; 2.0 4.0]
    @test typeof(m) === Matrix{Float64}
end

@testset "hcat preserves Matrix{String}" begin
    m = hcat(["a", "b"], ["c", "d"])
    @test size(m) == (2, 2)
    @test m[1, 1] == "a"
    @test m[2, 2] == "d"
    @test typeof(m) === Matrix{String}
end

@testset "hcat dimension mismatch still raises" begin
    @test_throws Exception hcat([1, 2], [3, 4, 5])
end

true
