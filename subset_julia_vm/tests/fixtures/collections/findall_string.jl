# findall(pattern, string) - find all non-overlapping occurrences (Issue #2013)
# Returns Vector of UnitRange{Int64} with 1-based byte indices.

using Test

@testset "findall(pattern, string) (Issue #2013)" begin
    # Basic string pattern
    r1 = findall("ll", "hello llama")
    @test length(r1) == 2
    @test first(r1[1]) == 3
    @test last(r1[1]) == 4
    @test first(r1[2]) == 7
    @test last(r1[2]) == 8

    # Multiple non-overlapping matches
    r2 = findall("ab", "ababab")
    @test length(r2) == 3
    @test first(r2[1]) == 1
    @test last(r2[1]) == 2
    @test first(r2[2]) == 3
    @test last(r2[2]) == 4
    @test first(r2[3]) == 5
    @test last(r2[3]) == 6

    # No matches
    r3 = findall("x", "abcdef")
    @test length(r3) == 0

    # Multiple matches with gaps
    r4 = findall("an", "banana")
    @test length(r4) == 2
    @test first(r4[1]) == 2
    @test last(r4[1]) == 3
    @test first(r4[2]) == 4
    @test last(r4[2]) == 5

    # Single character match
    r5 = findall("b", "abc")
    @test length(r5) == 1
    @test first(r5[1]) == 2
    @test last(r5[1]) == 2

    # Predicate form still works
    @test findall(x -> x > 3, [1, 2, 3, 4, 5]) == [4.0, 5.0]

    # Single-arg form still works
    @test findall([true, false, true]) == [1.0, 3.0]
end

true
