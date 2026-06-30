using Test

# Issue #5676: endswith(s, suffix::Regex) — true iff the pattern matches ENDING at
# the end of `s` (upstream uses PCRE ENDANCHORED). A leftmost `match` cannot decide
# this (e.g. endswith("hello", r".") is true though the leftmost match of `.` is at
# index 1), so a dedicated method delegating to the engine-anchored _endswith_regex
# builtin is required. The startswith(s, ::Regex) half shipped earlier (PR #5677).

@testset "endswith with a Regex suffix (Issue #5676)" begin
    @test endswith("hello123", r"\d+") == true
    @test endswith("hello123", r"\d") == true
    @test endswith("hello", r"\d+") == false
    @test endswith("hello", r"o") == true
    @test endswith("hello", r"l") == false
    @test endswith("hello", r"lo") == true
    @test endswith("abc", r"c$") == true
    @test endswith("abc", r"^abc$") == true
    @test endswith("hello", r".") == true          # leftmost-match check would fail here
    @test endswith("", r"") == true
    @test endswith("a.b.c", r"\.") == false
    @test endswith("a.b.c", r"c") == true
    @test endswith("aaa", r"aa") == true            # overlap: leftmost non-overlapping would miss
    @test endswith("baa", r"a") == true

    # Dynamic dispatch (suffix not statically typed).
    g(s, suf) = endswith(s, suf)
    @test g("hello123", r"\d+") == true
    @test g("hello", r"l") == false

    # startswith(s, ::Regex) still works (no regression).
    @test startswith("hello", r"he") == true
    @test startswith("hello", r"lo") == false
end

true
