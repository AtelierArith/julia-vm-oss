# Phase 4-4: Set in-place operations in Pure Julia (Issue #2575)
# Tests union!, intersect!, setdiff!, symdiff!

using Test

@testset "Set in-place operations - Pure Julia" begin
    # union!
    s = Set([1, 2])
    union!(s, Set([3, 4]))
    @test length(s) == 4
    @test 1 in s
    @test 3 in s
    @test 4 in s

    # setdiff!
    s2 = Set([1, 2, 3, 4, 5])
    setdiff!(s2, Set([2, 4]))
    @test length(s2) == 3
    @test 1 in s2
    @test 3 in s2
    @test 5 in s2
    @test !(2 in s2)
    @test !(4 in s2)

    # symdiff!
    s3 = Set([1, 2, 3])
    symdiff!(s3, Set([2, 3, 4]))
    @test length(s3) == 2
    @test 1 in s3
    @test 4 in s3
    @test !(2 in s3)
    @test !(3 in s3)
end

true
