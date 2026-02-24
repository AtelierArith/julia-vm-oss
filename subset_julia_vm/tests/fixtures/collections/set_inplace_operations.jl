# Test Set in-place operations: union!, intersect!, setdiff!, symdiff!
# Issue #1454: Implement Set in-place operations

using Test

@testset "Set in-place operations" begin
    @testset "union!" begin
        # union!(s, itr) - add elements from itr to s in-place
        s = Set([1, 2, 3])
        union!(s, Set([3, 4, 5]))
        @test length(s) == 5
        @test 1 in s
        @test 4 in s
        @test 5 in s

        # union! with array
        s2 = Set([10, 20])
        union!(s2, Set([20, 30, 40]))
        @test length(s2) == 4
    end

    @testset "intersect!" begin
        # intersect!(s, itr) - keep only elements also in itr
        s = Set([1, 2, 3, 4, 5])
        intersect!(s, Set([2, 3, 4, 6, 7]))
        @test length(s) == 3
        @test 2 in s
        @test 3 in s
        @test 4 in s
        @test !(1 in s)
        @test !(5 in s)
    end

    @testset "setdiff!" begin
        # setdiff!(s, itr) - remove elements found in itr
        s = Set([1, 2, 3, 4, 5])
        setdiff!(s, Set([2, 4]))
        @test length(s) == 3
        @test 1 in s
        @test 3 in s
        @test 5 in s
        @test !(2 in s)
        @test !(4 in s)
    end

    @testset "symdiff!" begin
        # symdiff!(s, itr) - symmetric difference in-place
        # Elements in s but not in itr stay, elements in both are removed,
        # elements in itr but not in s are added
        s = Set([1, 2, 3])
        symdiff!(s, Set([2, 3, 4]))
        @test length(s) == 2
        @test 1 in s
        @test 4 in s
        @test !(2 in s)
        @test !(3 in s)
    end

    @testset "combined operations" begin
        # Test chaining multiple in-place operations
        s = Set([1, 2, 3, 4, 5])
        union!(s, Set([6, 7]))
        @test length(s) == 7

        setdiff!(s, Set([1, 7]))
        @test length(s) == 5
        @test !(1 in s)
        @test !(7 in s)

        intersect!(s, Set([2, 3, 4, 10, 20]))
        @test length(s) == 3
        @test 2 in s
        @test 3 in s
        @test 4 in s
    end
end

true
