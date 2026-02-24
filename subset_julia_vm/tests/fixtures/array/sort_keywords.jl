# sort/sort! keyword arguments: by, rev, lt (Issue #2011)
# In Julia, sort supports keyword arguments for custom comparisons and transforms.

using Test

myabs(x) = abs(x)
mygt(a, b) = a > b

@testset "sort keyword arguments (Issue #2011)" begin
    # Default sort (ascending)
    @test sort([3, 1, 4, 1, 5]) == [1.0, 1.0, 3.0, 4.0, 5.0]

    # rev=true (descending)
    @test sort([3, 1, 4, 1, 5], rev=true) == [5.0, 4.0, 3.0, 1.0, 1.0]

    # by=abs (sort by absolute value)
    @test sort([-3, 1, -2], by=abs) == [1.0, -2.0, -3.0]

    # by + rev combined
    @test sort([-3, 1, -2], by=abs, rev=true) == [-3.0, -2.0, 1.0]

    # by with named function
    @test sort([-3, 1, -2], by=myabs) == [1.0, -2.0, -3.0]

    # lt with named function (custom comparison: descending)
    @test sort([3, 1, 4, 1, 5], lt=mygt) == [5.0, 4.0, 3.0, 1.0, 1.0]

    # sort! in-place with rev
    a = [5, 3, 1, 4, 2]
    sort!(a, rev=true)
    @test a == [5.0, 4.0, 3.0, 2.0, 1.0]

    # sort! in-place with by
    b = [-3, 1, -2, 4]
    sort!(b, by=abs)
    @test b == [1.0, -2.0, -3.0, 4.0]
end

true
