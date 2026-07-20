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

# Upstream rsplit only has the `limit=` KEYWORD form; the positional
# 3-arg spelling is an sjulia-internal helper (Issues #10324 / #10237).
@testset "rsplit with limit" begin
    # limit=2: only 1 split from the right
    @test rsplit("M.a.r.c.h", "."; limit=2) == ["M.a.r.c", "h"]

    # limit=3: 2 splits from the right
    @test rsplit("M.a.r.c.h", "."; limit=3) == ["M.a.r", "c", "h"]

    # limit=1: no splits, return whole string
    @test rsplit("a.b.c", "."; limit=1) == ["a.b.c"]

    # limit >= number of parts: same as no limit
    @test rsplit("a.b.c", "."; limit=10) == ["a", "b", "c"]

    # limit=0: same as no limit
    @test rsplit("a.b.c", "."; limit=0) == ["a", "b", "c"]

    # Char delimiter with limit
    @test rsplit("one-two-three-four", '-'; limit=2) == ["one-two-three", "four"]
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

# Upstream exposes only the `limit=` KEYWORD form; the positional 3-arg spelling
# rsplit(s, delim, limit) is a MethodError upstream and must not be reachable
# (Issue #10324 item 2).
@testset "rsplit positional 3-arg form is a MethodError" begin
    @test_throws MethodError rsplit("a.b.c", ".", 2)
    @test_throws MethodError rsplit("a-b-c", '-', 2)
end

true
