# Test rsplit function - reverse split string by delimiter (Issue #1992)
# rsplit without limit behaves like split.
# rsplit with limit splits from the right, keeping leftmost parts together.

using Test

@testset "rsplit basic and Char delimiter" begin
    # Basic split by string delimiter (same as split without limit)
    @test rsplit("a.b.c", ".") == ["a", "b", "c"]
    @test rsplit("hello::world::test", "::") == ["hello", "world", "test"]

    # No delimiter found
    @test rsplit("hello", ",") == ["hello"]

    # Empty string between delimiters
    @test rsplit("a,,b", ",") == ["a", "", "b"]

    # Delimiter at start and end
    @test rsplit(",a,b,", ",") == ["", "a", "b", ""]

    # Empty string
    @test rsplit("", ",") == [""]

    # Char delimiter
    @test rsplit("x-y-z", '-') == ["x", "y", "z"]
end

@testset "rsplit with limit" begin
    # limit=2: only 1 split from the right
    @test rsplit("M.a.r.c.h", ".", 2) == ["M.a.r.c", "h"]

    # limit=3: 2 splits from the right
    @test rsplit("M.a.r.c.h", ".", 3) == ["M.a.r", "c", "h"]

    # limit=1: no splits, return whole string
    @test rsplit("a.b.c", ".", 1) == ["a.b.c"]

    # limit >= number of parts: same as no limit
    @test rsplit("a.b.c", ".", 10) == ["a", "b", "c"]

    # limit=0: same as no limit
    @test rsplit("a.b.c", ".", 0) == ["a", "b", "c"]

    # Char delimiter with limit
    @test rsplit("one-two-three-four", '-', 2) == ["one-two-three", "four"]
end

@testset "rsplit matches split without limit" begin
    s = "one-two-three"
    sr = split(s, "-")
    rr = rsplit(s, "-")
    @test length(sr) == length(rr)
    @test isequal(sr[1], rr[1])
    @test isequal(sr[2], rr[2])
    @test isequal(sr[3], rr[3])
end

true
