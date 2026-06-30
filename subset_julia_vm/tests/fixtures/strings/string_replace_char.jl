using Test

# Regression test for Issue #3596:
# `replace(s, old => new)` must accept Char arguments on either side of the
# pair. Previously the impl called `length(old)` directly which failed with
# "length not defined for Char".

@testset "replace with Char pairs (#3596)" begin
    # Char => Char (MWE from issue)
    @test replace("aba", 'a' => 'x') == "xbx"

    # Char => String
    @test replace("aba", 'a' => "xx") == "xxbxx"

    # String => Char
    @test replace("aba", "a" => 'x') == "xbx"

    # All-Char on a longer string
    @test replace("hello world", 'l' => 'L') == "heLLo worLd"

    # No match for the Char
    @test replace("hello", 'z' => 'Z') == "hello"

    # Empty target string
    @test replace("", 'a' => 'b') == ""

    # Char with count keyword
    @test replace("aaaa", 'a' => 'b'; count=2) == "bbaa"

    # Existing String=>String still works (regression check)
    @test replace("hello", "ll" => "LL") == "heLLo"
end

true
