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

    # count=0: replace NOTHING (upstream semantics — Issues #10197 / #10237)
    @test replace("aabaa", "a" => "x", count=0) == "aabaa"

    # negative count: DomainError (upstream semantics — Issue #10197)
    @test_throws DomainError replace("aabaa", "a" => "x", count=-1)

    # No count: replace all
    @test replace("aabaa", "a" => "x") == "xxbxx"

    # count larger than number of matches
    @test replace("ab", "a" => "x", count=10) == "xb"

    # Multi-char pattern with count
    @test replace("abcabcabc", "abc" => "X", count=2) == "XXabc"
end

@testset "replace rejects bad operands even at count=0 (Issue #10237)" begin
    # The count=0 short-circuit must NOT bypass operand validation: a
    # non-string receiver is a MethodError under upstream at every count,
    # including 0 (previously sjulia silently returned the receiver).
    @test_throws MethodError replace(42, "a" => "x", count=0)
    @test_throws MethodError replace(42, "a" => "x", count=2)
    # A valid receiver + pair with count=0 still returns it unchanged.
    @test replace("aabaa", "a" => "x", count=0) == "aabaa"
end

true
