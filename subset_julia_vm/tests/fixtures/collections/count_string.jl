# count(pattern, string) - count non-overlapping occurrences of pattern in string (Issue #2009)
# In Julia, count(pattern, string) counts how many times pattern appears in string.

using Test

@testset "count(pattern, string) (Issue #2009)" begin
    # Basic string pattern counting
    @test count("ab", "ababab") == 3
    @test count("hello", "hello world hello") == 2
    @test count("x", "abcdef") == 0
    @test count("a", "banana") == 3

    # Char pattern counting
    @test count('a', "banana") == 3
    @test count('z', "banana") == 0
    @test count('l', "hello world") == 3

    # Non-overlapping: "aa" in "aaaa" should be 2, not 3
    @test count("aa", "aaaa") == 2

    # Single character match
    @test count("b", "abc") == 1

    # Full string match
    @test count("abc", "abc") == 1

    # Empty result
    @test count("xyz", "abc") == 0

    # Predicate form still works
    @test count(x -> x > 3, [1, 2, 3, 4, 5]) == 2

    # Single-arg count still works
    @test count([true, false, true, true]) == 3
end

true
