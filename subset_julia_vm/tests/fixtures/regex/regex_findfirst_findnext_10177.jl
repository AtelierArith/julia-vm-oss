# Issue #10177: findfirst(r::Regex, s) and findnext(r::Regex, s, i) return the
# 1-based byte UnitRange of the next match (or `nothing`). These pure-Julia
# wrappers in base/strings/search.jl are built on the `_regex_findnext` builtin,
# the sjulia analog of upstream's `PCRE.exec(re, str, idx-1)` positional search:
# it runs against the FULL string, so overlapping matches and `^` / `\b` /
# lookbehind context behave like upstream. `findlast(::Regex, s)` is NOT defined
# — upstream Julia itself throws MethodError for it. All values verified against
# upstream julia 1.12.

using Test

@testset "findfirst(::Regex, s) (Issue #10177)" begin
    @test findfirst(r"\d+", "ab12cd34") == 3:4
    @test findfirst(r"\d", "a1b2c3") == 2:2
    @test findfirst(r"(\d)(\d)", "ab12cd") == 3:4
    @test findfirst(r"x", "abc") === nothing        # no match
    @test findfirst(r"", "abc") == 1:0              # empty pattern → empty range
    @test findfirst(r"\d", "αβ1γ2") == 5:5          # unicode haystack (byte offset)
end

@testset "findnext(::Regex, s, i) (Issue #10177)" begin
    @test findnext(r"\d+", "ab12cd34", 5) == 7:8    # skips the first match
    @test findnext(r"\d", "a1b2c3", 3) == 4:4
    @test findnext(r"\d\d", "123", 2) == 2:3        # overlapping match (positional)
    @test findnext(r"\d", "abc", 4) === nothing     # i == ncodeunits(s)+1, no match
    @test findnext(r"", "abc", 2) == 2:1            # empty pattern at position i
    @test findnext(r"\d", "αβ1γ2", 6) == 8:8        # unicode haystack
    @test findnext(r"\w+", "  foo bar", 4) == 4:5   # search resumes inside "foo"
end

@testset "findnext(::Regex, s, i) context / bounds (Issue #10177)" begin
    # `^` anchors to the true start of the full string, not to position i, so a
    # positional search from i=2 finds no match (context is preserved).
    @test findnext(r"^\d", "a1", 2) === nothing
    # idx beyond nextind(s, lastindex(s)) throws BoundsError, mirroring upstream.
    @test_throws BoundsError findnext(r"\d", "abc", 10)
    # findlast(::Regex, s) is intentionally undefined (upstream throws MethodError).
    @test_throws MethodError findlast(r"\d", "abc")
end

true
