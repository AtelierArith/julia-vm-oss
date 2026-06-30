# Issue #8314: a comparison operator in the then-branch of a ternary must parse
# (the `:` after it is the ternary separator, not a range). Genuine ranges inside
# a grouping in the then-branch must still work.

using Test

@testset "comparison in ternary then-branch (Issue #8314)" begin
    # then-branch comparison: `:` is the separator
    @test (true ? 1 > 0 : 2 > 0) == true
    @test (false ? 1 > 0 : 2 > 0) == true
    @test (true ? 1 == 0 : 2 == 0) == false
    # comparison in the condition still works
    @test (1 < 2 ? 3 : 4) == 3
    # nested ternary with a comparison in the then-branch
    @test (true ? 2 > 1 ? 10 : 20 : 30) == 10
end

@testset "parenthesized range in ternary then-branch (Issue #8314)" begin
    # a real range inside a grouping in the then-branch is unaffected
    @test (true ? (1 : 3) : (4 : 6)) == 1:3
    @test (false ? (1 : 3) : (4 : 6)) == 4:6
    @test collect(true ? (1:3) : (4:6)) == [1, 2, 3]
end

true
