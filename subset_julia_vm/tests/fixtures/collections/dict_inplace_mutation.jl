# Dict in-place mutation: empty! and merge! (Issue #2134)
# Verifies that empty!(d) and merge!(d1, d2) modify the dict in-place.

using Test

@testset "empty!(dict) clears all entries in-place (Issue #2134)" begin
    d = Dict("a" => 1, "b" => 2, "c" => 3)
    @test length(d) == 3
    empty!(d)
    @test length(d) == 0
    @test isempty(d)
end

@testset "merge!(dict1, dict2) merges in-place (Issue #2134)" begin
    d1 = Dict("a" => 1)
    d2 = Dict("b" => 2, "c" => 3)
    merge!(d1, d2)
    @test length(d1) == 3
    @test haskey(d1, "a")
    @test haskey(d1, "b")
    @test haskey(d1, "c")
    @test d1["b"] == 2
end

@testset "empty!(dict) returns the emptied dict (Issue #2134)" begin
    d = Dict("x" => 10)
    result = empty!(d)
    @test length(result) == 0
    @test length(d) == 0
end

@testset "merge!(dict1, dict2) overwrites existing keys (Issue #2134)" begin
    d1 = Dict("a" => 1, "b" => 2)
    d2 = Dict("b" => 99, "c" => 3)
    merge!(d1, d2)
    @test d1["b"] == 99
    @test d1["c"] == 3
    @test length(d1) == 3
end

true
