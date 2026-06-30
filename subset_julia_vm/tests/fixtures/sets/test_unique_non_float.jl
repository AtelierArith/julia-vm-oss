using Test

# Regression tests for Issues #3581, #3586:
# `unique(arr)` and `unique(f, arr)` must work for arrays of non-Float64
# element types (String, Bool, Int, etc.). Previously they pre-allocated
# `result = zeros(count)` which made the function unusable for any array
# whose elements weren't representable as Float64.
#
# Note: full type-preservation is blocked on #3648; the result element type
# here is currently `Any` rather than the input element type, but the values
# are correct and downstream `==` / `length` / iteration work as expected.

@testset "unique on Vector{String} (#3581)" begin
    x = unique(["apple", "banana", "apple", "cherry"])
    @test x == ["apple", "banana", "cherry"]
    @test length(x) == 3

    # All-duplicates collapses to one
    y = unique(["a", "a", "a"])
    @test y == ["a"]
    @test length(y) == 1

    # Single element
    @test unique(["x"]) == ["x"]
end

@testset "unique on Vector{Bool}" begin
    @test unique([true, false, true]) == [true, false]
    @test unique([true, true, true]) == [true]
end

@testset "unique on Vector{Int} preserves values (#3580 partial)" begin
    # Values are preserved correctly; element type is Any (full type-preservation
    # is blocked on #3648, tracked separately).
    x = unique([1, 2, 1, 3])
    @test x == [1, 2, 3]
end

@testset "unique(f, arr) for non-numeric (#3586)" begin
    # length-based unique on strings: keeps first occurrence per length
    x = unique(length, ["a", "bb", "c", "dd", "eee"])
    @test x == ["a", "bb", "eee"]

    # Identity function on strings
    @test unique(identity, ["a", "b", "a"]) == ["a", "b"]
end

true
