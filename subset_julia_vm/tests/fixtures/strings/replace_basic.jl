# Test replace - replace occurrences of old with new
# Uses Julia-compatible Pair syntax: replace(s, old => new; count=N)

using Test

@testset "replace(s, old => new) - Pure Julia (Issue #682)" begin
    @test replace("hello world", "world" => "Julia") == "hello Julia"
    @test replace("aaa", "a" => "bb") == "bbbbbb"
    @test replace("abc", "x" => "y") == "abc"
end

@testset "replace with count keyword (Issue #2043)" begin
    # count=1: replace only the first occurrence
    @test replace("aabaa", "a" => "x", count=1) == "xabaa"

    # count=2: replace first two occurrences
    @test replace("aabaa", "a" => "x", count=2) == "xxbaa"

    # count=3: replace first three occurrences
    @test replace("aabaa", "a" => "x", count=3) == "xxbxa"

    # count=0: replace all (same as default)
    @test replace("aabaa", "a" => "x", count=0) == "xxbxx"

    # No count: replace all
    @test replace("aabaa", "a" => "x") == "xxbxx"

    # count larger than number of matches
    @test replace("ab", "a" => "x", count=10) == "xb"

    # Multi-char pattern with count
    @test replace("abcabcabc", "abc" => "X", count=2) == "XXabc"
end

true
