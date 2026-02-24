# Test findnext and findprev string search functions

using Test

@testset "findnext/findprev - find next/previous occurrence in string" begin

    # findnext with character (returns Int64 or nothing)
    result1 = findnext('a', "abcabc", 1)
    @test result1 == 1

    result2 = findnext('a', "abcabc", 2)
    @test result2 == 4

    result3 = findnext('a', "abcabc", 5)
    @test result3 === nothing

    result4 = findnext('z', "abcabc", 1)
    @test result4 === nothing

    # findnext with substring (returns UnitRange{Int64} or nothing)
    result5 = findnext("bc", "abcabc", 1)
    @test first(result5) == 2
    @test last(result5) == 3

    result6 = findnext("bc", "abcabc", 3)
    @test first(result6) == 5
    @test last(result6) == 6

    result7 = findnext("xyz", "abcabc", 1)
    @test result7 === nothing

    # findprev with character (returns Int64 or nothing)
    result8 = findprev('a', "abcabc", 6)
    @test result8 == 4

    result9 = findprev('a', "abcabc", 3)
    @test result9 == 1

    result10 = findprev('z', "abcabc", 6)
    @test result10 === nothing

    # findprev with substring (returns UnitRange{Int64} or nothing)
    result11 = findprev("ab", "abcabc", 6)
    @test first(result11) == 4
    @test last(result11) == 5

    result12 = findprev("ab", "abcabc", 3)
    @test first(result12) == 1
    @test last(result12) == 2
end

true  # Test passed
