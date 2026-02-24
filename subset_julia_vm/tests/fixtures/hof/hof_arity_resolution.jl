using Test

# Issue #3125: HOF function resolution arity mismatch
# Tests sprint/show type patterns and arity-specific HOF dispatch

@testset "sprint with 2-arg show function" begin
    # show(io, x) is a 2-arg function, sprint calls it as f(io, args...)
    @test sprint(show, 42) == "42"
    @test sprint(show, "hello") == "\"hello\""
    @test sprint(show, [1, 2, 3]) == "[1, 2, 3]"
    @test sprint(show, (1, 2)) == "(1, 2)"
end

@testset "sprint with print - different arity" begin
    # print(io, x) takes 2 args, but also print(io, x, y...) takes varargs
    @test sprint(print, 42) == "42"
    @test sprint(print, "a", "b") == "ab"
    @test sprint(print, 1, 2, 3) == "123"
end

@testset "map with single-arg functions" begin
    @test map(string, [1, 2, 3]) == ["1", "2", "3"]
    @test map(typeof, [1, 2.0, "a"]) == [Int64, Float64, String]
    @test map(length, ["ab", "cde", "f"]) == [2, 3, 1]
end

@testset "map with multi-arg function" begin
    @test map(+, [1, 2, 3], [10, 20, 30]) == [11, 22, 33]
    @test map(*, [2, 3], [4, 5]) == [8, 15]
end

@testset "filter with single-arg predicate" begin
    @test filter(isodd, [1, 2, 3, 4, 5]) == [1, 3, 5]
    @test filter(iseven, [1, 2, 3, 4, 5]) == [2, 4]
end

true
