using Test

# Issue #10080: PCRE2-vs-fancy-regex parity audit — eachmatch iteration edge
# cases (zero-width matches, non-overlapping offsets), PCRE character classes
# shared by both engines (\R any newline, \N not-a-newline, \xHH hex
# escape), and lazy / counted quantifiers.
# NOT covered here (known gaps): \h/\H match any character (Issue #10203),
# \v class semantics (Issue #10180), eachmatch(...; overlap=true) is
# silently ignored (Issue #10199), and collect(eachmatch(...)) errors
# (Issue #10198) — comprehensions are used instead. Verified against
# upstream julia 1.12.

@testset "regex eachmatch iteration (Issue #10080)" begin
    @test [m.match for m in eachmatch(r"a.", "abab")] == ["ab", "ab"]
    # Zero-width pattern yields a match at every boundary.
    @test length([m for m in eachmatch(r"", "ab")]) == 3
    @test [m.offset for m in eachmatch(r"aa", "aaaa")] == [1, 3]
end

@testset "regex PCRE character classes (Issue #10080)" begin
    @test occursin(r"\R", "a\r\nb") == true
    @test occursin(r"\R", "ab") == false
    @test match(r"\N+", "ab\ncd").match == "ab"
    @test occursin(r"\x41", "A") == true
end

@testset "regex lazy and counted quantifiers (Issue #10080)" begin
    @test match(r"<.+?>", "<a><b>").match == "<a>"
    @test match(r"a{2,3}", "aaaa").match == "aaa"
    @test match(r"a{2,}?", "aaaa").match == "aa"
    @test match(r"a{4}", "aaa") === nothing
end

true
