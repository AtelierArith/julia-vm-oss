using Test

# Issue #5663: the 3-argument `join(itr, delim, last)` form uses a distinct
# separator `last` before the FINAL element — `join([1,2,3], ", ", " and ")` is
# "1, 2 and 3". sjulia only had the 1- and 2-argument `join` methods, so the
# 3-argument call failed with NoMethodFound.

@testset "join(itr, delim, last) uses a distinct final separator (Issue #5663)" begin
    @test join([1, 2, 3], ", ", " and ") == "1, 2 and 3"
    @test join([1, 2], ", ", " and ") == "1 and 2"
    @test join(["a", "b", "c", "d"], ", ", " or ") == "a, b, c or d"
    @test join(["x", "y"], "-", " & ") == "x & y"

    # Edge cases: single element ignores both separators; empty is "".
    @test join([42], ", ", " and ") == "42"
    @test join(String[], ", ", " and ") == ""

    # Works over a range, and the result is an ordinary String.
    @test join(1:3, ", ", " and ") == "1, 2 and 3"
    @test length(join([1, 2, 3], ", ", " and ")) == 10

    # The 1- and 2-argument forms are unchanged.
    @test join([1, 2, 3], ", ") == "1, 2, 3"
    @test join(["a", "b", "c"]) == "abc"
end

true
