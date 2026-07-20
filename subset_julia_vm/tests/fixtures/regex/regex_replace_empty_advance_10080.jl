using Test

# Issue #10080: PCRE2-vs-fancy-regex parity audit — replace() edge cases that
# already match upstream Julia: empty-match advancement (the engine must step
# one character after a zero-width match instead of looping) and the count
# keyword. Capture references in s"..." are NOT covered here (Issue #10174).
# Verified against upstream julia 1.12.

@testset "regex replace empty-match advancement (Issue #10080)" begin
    @test replace("abc", r"" => "-") == "-a-b-c-"
    @test replace("abc", r"x*" => "-") == "-a-b-c-"
    @test replace("", r"" => "X") == "X"
end

@testset "regex replace count kwarg (Issue #10080)" begin
    @test replace("aaa", r"a" => "b"; count=2) == "bba"
    @test replace("aaa", r"a" => "b"; count=1) == "baa"
    # count=0 is NOT covered: sjulia replaces all instead of none (Issue #10197).
    @test replace("a1b2", r"\d" => "#") == "a#b#"
end

true
