using Test

# Issue #10181: PCRE2 pattern-recursion / subroutine-call constructs are not
# supported by the fancy-regex engine. Upstream Julia (PCRE2) runs them, but
# fancy-regex either fails to compile `(?1)` with an opaque "Unknown group flag"
# message or — worse — compiles `(?R)` to something else and SILENTLY returns a
# wrong match. Until real recursion support exists, sjulia rejects every
# recursion construct at Regex construction with a clear, documented error.
#
# NOTE: this fixture is a deliberate sjulia-vs-upstream DIVERGENCE — the same
# patterns compile and match under upstream julia — so it is marked
# `skip_julia_test = true` in manifest.toml and MUST NOT be run under upstream
# julia. See docs/vm/REGEX_PCRE2_PARITY.md.

@testset "regex recursion constructs are rejected (Issue #10181)" begin
    # Whole-pattern recursion.
    @test_throws "recursion" r"\((?:[^()]|(?R))*\)"
    @test_throws "recursion" r"(?0)"
    # Numbered / relative subroutine calls.
    @test_throws "recursion" r"^(x(?1)?y)$"
    @test_throws "recursion" r"(?12)"
    @test_throws "recursion" r"(a)(?+1)"
    @test_throws "recursion" r"(a)(?-1)"
    # Named subroutine calls (PCRE `(?&name)` and Python `(?P>name)`).
    @test_throws "recursion" r"(?<a>x)(?&a)"
    @test_throws "recursion" r"(?P<a>x)(?P>a)"
end

@testset "recursion look-alikes still compile (Issue #10181)" begin
    # None of these are recursion; the rejection must not over-reach.
    @test occursin(r"(?:a|b)+c", "abc") == true      # non-capturing group
    @test occursin(r"(?i)abc", "ABC") == true        # inline flag
    @test occursin(r"(?-i)abc", "abc") == true       # inline flag negation
    @test match(r"(?>a+)b", "aaab").match == "aaab"  # atomic group
    @test occursin(r"(?=abc)", "abc") == true        # lookahead
    @test occursin(r"(?<=ab)c", "abc") == true       # lookbehind
    @test match(r"(?<name>\d+)", "x42").match == "42" # named capture (PCRE)
    @test match(r"(?P<name>\d+)", "x42").match == "42" # named capture (Python)
    @test occursin(r"(a)?(?(1)b|c)", "c") == true    # conditional group
    @test occursin(r"a(?#comment)b", "ab") == true   # comment group
    # An escaped paren / a class containing the token text is not a group.
    @test occursin(r"\(\?R\)", "(?R)") == true
    @test occursin(r"[(?R)]+", "R?()") == true
end

true
