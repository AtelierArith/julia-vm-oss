using Test

# Issue #5676: `startswith(s, re::Regex)` — true iff the regex matches at the
# START of the string. sjulia only handled string prefixes ("expected String,
# got Regex"). Implemented as: the regex's leftmost match (from `match`) begins at
# index 1, equivalent to upstream's start-anchored match.

@testset "startswith with a Regex pattern (Issue #5676)" begin
    @test startswith("hello", r"h.") == true
    @test startswith("hello", r"x") == false
    @test startswith("hello", r"l") == false      # matches at 3, not the start
    @test startswith("hello", r".*o") == true
    @test startswith("hello", r"he|xy") == true    # alternation, matches at start
    @test startswith("abc", r"[a-c]") == true
    @test startswith("123abc", r"\d+") == true
    @test startswith("abc123", r"\d+") == false
end

@testset "string startswith is unchanged (Issue #5676)" begin
    @test startswith("hello", "he") == true
    @test startswith("hello", "x") == false
    @test startswith("hello", "") == true
end

true
