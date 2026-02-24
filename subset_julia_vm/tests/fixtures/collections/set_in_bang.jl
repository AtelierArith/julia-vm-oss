# Test in! function for Set
# Issue #1459: Implement in! function for Set

using Test

@testset "in! function" begin
    @testset "basic in! operations" begin
        # in! returns false when element is not in set (and adds it)
        s = Set([1, 2, 3])
        result = in!(4, s)
        @test result == false
        @test length(s) == 4
        @test 4 in s

        # in! returns true when element is already in set
        result2 = in!(4, s)
        @test result2 == true
        @test length(s) == 4  # length unchanged
    end

    @testset "in! with existing elements" begin
        s = Set([10, 20, 30])

        # Check existing elements
        @test in!(10, s) == true
        @test in!(20, s) == true
        @test in!(30, s) == true
        @test length(s) == 3  # no new elements added
    end

    @testset "in! with new elements" begin
        s = Set([1])

        # Add multiple new elements
        @test in!(2, s) == false
        @test in!(3, s) == false
        @test in!(4, s) == false
        @test length(s) == 4

        # Now all should return true
        @test in!(1, s) == true
        @test in!(2, s) == true
        @test in!(3, s) == true
        @test in!(4, s) == true
    end

    @testset "in! with empty set" begin
        s = Set{Int64}()
        @test in!(1, s) == false
        @test length(s) == 1
        @test 1 in s
    end
end

true
