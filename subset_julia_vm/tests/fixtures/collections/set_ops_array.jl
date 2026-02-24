# Set operations on arrays: union, intersect, setdiff, symdiff (Issue #2007)
# In Julia, these functions work with any iterables, not just Sets.

using Test

@testset "set operations on arrays (Issue #2007)" begin
    # intersect: elements in both arrays
    @test intersect([1, 2, 3], [2, 3, 4]) == [2.0, 3.0]
    @test length(intersect([1, 2, 3], [4, 5])) == 0

    # setdiff: elements in first but not second
    @test setdiff([1, 2, 3], [2]) == [1.0, 3.0]
    @test length(setdiff([1, 2, 3], [1, 2, 3])) == 0

    # symdiff: elements in one but not both (symmetric difference)
    @test symdiff([1, 2, 3], [2, 3, 4]) == [1.0, 4.0]

    # union: all unique elements from both arrays
    @test union([1, 2, 3], [3, 4, 5]) == [1.0, 2.0, 3.0, 4.0, 5.0]

    # Duplicate handling
    @test union([1, 1, 2], [2, 3]) == [1.0, 2.0, 3.0]
    @test intersect([1, 1, 2, 3], [2, 3, 3, 4]) == [2.0, 3.0]
end

true
