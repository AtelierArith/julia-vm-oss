# Test string split function
# Based on Julia's base/strings/util.jl

using Test

@testset "String split function" begin
    # Basic split with string delimiter
    @test split("a,b,c", ",") == ["a", "b", "c"]
    @test split("hello world", " ") == ["hello", "world"]

    # Split with multi-character delimiter
    @test split("a::b::c", "::") == ["a", "b", "c"]

    # Split with no delimiter matches
    @test split("hello", ",") == ["hello"]

    # Split at beginning
    @test split(",a,b", ",") == ["", "a", "b"]

    # Split at end
    @test split("a,b,", ",") == ["a", "b", ""]

    # Multiple consecutive delimiters
    @test split("a,,b", ",") == ["a", "", "b"]

    # Empty string
    @test split("", ",") == [""]

    # Single character split
    @test split("abc", "") == ["a", "b", "c"]

    # Split with Char delimiter
    @test split("a-b-c", '-') == ["a", "b", "c"]
end

@testset "String split with limit keyword (Issue #2040)" begin
    # limit=2: split at most once
    @test split("a-b-c-d", "-", limit=2) == ["a", "b-c-d"]

    # limit=3: split at most twice
    @test split("a-b-c-d", "-", limit=3) == ["a", "b", "c-d"]

    # limit=1: no split at all
    @test split("a-b-c-d", "-", limit=1) == ["a-b-c-d"]

    # limit=0: no limit (same as default)
    @test split("a-b-c-d", "-", limit=0) == ["a", "b", "c", "d"]

    # limit with space delimiter
    @test split("hello world foo bar", " ", limit=2) == ["hello", "world foo bar"]

    # limit with Char delimiter
    @test split("a-b-c-d", '-', limit=2) == ["a", "b-c-d"]

    # limit greater than number of parts: returns all parts
    @test split("a-b", "-", limit=10) == ["a", "b"]
end

true
