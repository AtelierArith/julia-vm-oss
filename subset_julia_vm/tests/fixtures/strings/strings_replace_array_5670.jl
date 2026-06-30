using Test

# Issue #5670: `replace(collection, old => new, ...)` over an array replaces each
# ELEMENT matching a pair's first value (by `isequal`) with its second. sjulia
# only had the string `replace`, so the array form failed with NoMethodFound.

@testset "replace over an array substitutes matching elements (Issue #5670)" begin
    @test replace([1, 2, 3, 2], 2 => 20) == [1, 20, 3, 20]
    @test replace([1, 2, 3, 2], 2 => 20, 3 => 30) == [1, 20, 30, 20]
    @test replace([1, 2, 3], 5 => 50) == [1, 2, 3]          # no match
    @test replace(["a", "b", "a"], "a" => "X") == ["X", "b", "X"]

    # Matching is by equality, not predicate: no element equals the function.
    @test replace([1, 2, 3, 4], iseven => 0) == [1, 2, 3, 4]

    # The original array is not mutated.
    v = [1, 2, 3]
    @test replace(v, 2 => 99) == [1, 99, 3]
    @test v == [1, 2, 3]
end

@testset "string replace is unchanged (Issue #5670)" begin
    @test replace("hello", 'l' => 'L') == "heLLo"
    @test replace("aaa", "a" => "b") == "bbb"
    @test replace("hello world", "o" => "0", count=1) == "hell0 world"
end

true
