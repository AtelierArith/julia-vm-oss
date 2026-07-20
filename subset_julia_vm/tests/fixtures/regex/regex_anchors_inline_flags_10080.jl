using Test

# Issue #10080: PCRE2-vs-fancy-regex parity audit — anchors and flag syntax
# that already match upstream Julia: \A/\z/\Z/\G anchors, inline (?i)(?s)(?m)
# flags (whole-pattern, scoped group, and mid-pattern), the x (extended) flag,
# and (?#...) comment groups. Verified against upstream julia 1.12.

@testset "regex string anchors (Issue #10080)" begin
    @test occursin(r"\Aabc", "abc\ndef") == true
    @test occursin(r"\Adef", "abc\ndef") == false
    @test occursin(r"def\z", "abc\ndef") == true
    @test occursin(r"def\z", "abc\ndef\n") == false
    # \Z also matches before a final newline.
    @test occursin(r"def\Z", "abc\ndef\n") == true
    @test occursin(r"\Gab", "abab") == true
end

@testset "regex inline flags (Issue #10080)" begin
    @test occursin(r"(?i)abc", "ABC") == true
    @test occursin(r"(?i:abc)d", "ABCd") == true
    @test occursin(r"(?i:abc)d", "ABCD") == false
    @test occursin(r"(?s).", "\n") == true
    @test occursin(r"(?m)^b", "a\nb") == true
    # Mid-pattern flag switch applies only to the remainder.
    @test occursin(r"a(?i)b", "aB") == true
    @test occursin(r"a(?i)b", "Ab") == false
end

@testset "regex extended flag and comment groups (Issue #10080)" begin
    @test match(r"(\d+) \s+ (\d+)"x, "12   34").match == "12   34"
    @test occursin(r"a b"x, "ab") == true
    @test occursin(r"a(?#this is a comment)b", "ab") == true
end

true
