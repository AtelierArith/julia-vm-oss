# Test eachrsplit function - reverse string split iterator (Issue #1994)
# eachrsplit yields substrings from right to left.

using Test

@testset "eachrsplit basic" begin
    # Basic right-to-left split
    @test collect(eachrsplit("a.b.c", ".")) == ["c", "b", "a"]
    @test collect(eachrsplit("hello::world::test", "::")) == ["test", "world", "hello"]

    # No delimiter found - returns whole string
    @test collect(eachrsplit("hello", ",")) == ["hello"]

    # Single element
    @test collect(eachrsplit("one", ".")) == ["one"]
end

@testset "eachrsplit with Char delimiter" begin
    @test collect(eachrsplit("x-y-z", '-')) == ["z", "y", "x"]
    @test collect(eachrsplit("a,b,c,d", ',')) == ["d", "c", "b", "a"]
end

@testset "eachrsplit reverse of eachsplit" begin
    # eachrsplit yields in reverse order compared to eachsplit
    s = "one.two.three"
    forward = collect(eachsplit(s, "."))
    backward = collect(eachrsplit(s, "."))
    @test length(forward) == length(backward)
    @test isequal(forward[1], backward[3])
    @test isequal(forward[2], backward[2])
    @test isequal(forward[3], backward[1])
end

true
