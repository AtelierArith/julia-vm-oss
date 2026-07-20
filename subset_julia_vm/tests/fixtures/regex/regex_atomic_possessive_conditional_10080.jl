using Test

# Issue #10080: PCRE2-vs-fancy-regex parity audit — backtracking-control
# constructs supported by both PCRE2 (upstream Julia) and fancy-regex:
# atomic groups (?>...), possessive quantifiers (a*+ / a++), \K keep-out,
# and conditional groups (?(n)yes|no). Verified against upstream julia 1.12.

@testset "regex atomic groups (Issue #10080)" begin
    @test match(r"(?>a+)b", "aaab").match == "aaab"
    # The atomic group refuses to give back an 'a', so this cannot match.
    @test match(r"(?>a+)ab", "aaab") === nothing
end

@testset "regex possessive quantifiers (Issue #10080)" begin
    @test match(r"a*+b", "aaab").match == "aaab"
    @test match(r"a*+ab", "aaab") === nothing
    @test match(r"\d++x", "123x").match == "123x"
end

@testset "regex \\K keep-out (Issue #10080)" begin
    m = match(r"foo\Kbar", "foobar")
    @test m !== nothing
    @test m.match == "bar"
    @test m.offset == 4
end

@testset "regex conditional groups (Issue #10080)" begin
    @test match(r"(a)?(?(1)b|c)", "ab").match == "ab"
    @test match(r"(a)?(?(1)b|c)", "c").match == "c"
end

true
