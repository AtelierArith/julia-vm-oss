using Test

# Issue #5709: flag-suffixed regex literals (r"..."i, r"..."ims) failed to parse —
# the parser captured only [prefix, string] and left the trailing flag chars as a
# separate identifier token. The parser now captures an adjacent identifier suffix
# (for the `r` prefix) as a third child, and lowering passes it as the Regex flags.

@testset "regex literal flags (Issue #5709)" begin
    # i: case-insensitive
    @test occursin(r"abc"i, "xABCy") == true
    @test match(r"abc"i, "xABCy").match == "ABC"
    @test occursin(r"\d"i, "A1") == true
    @test occursin(r"HELLO"i, "hello") == true
    @test occursin(r"abc"i, "xyz") == false

    # m: multiline (^ and $ match line boundaries)
    @test (match(r"^foo$"m, "bar\nfoo\nbaz") !== nothing) == true

    # s: dotall (. matches newline)
    @test occursin(r"a.b"s, "a\nb") == true
    @test occursin(r"a.b", "a\nb") == false

    # No flags still works (no regression).
    @test occursin(r"\d+", "a12") == true
    @test match(r"abc", "abc").match == "abc"

    # endswith / startswith with a flagged regex (composes with #5676 / #5677).
    @test endswith("fooBAR", r"bar"i) == true
    @test startswith("BARfoo", r"bar"i) == true
end

true
