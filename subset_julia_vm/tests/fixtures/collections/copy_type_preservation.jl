# Test that copy() preserves type for all collection types (Issue #1829)
#
# This test ensures that copy(x) returns a value of the same type as x
# for all supported collection types. This prevents regressions where
# a generic copy() fallback might return a different type (e.g., Vector
# instead of the original collection type).

using Test

@testset "copy() preserves type for all collections" begin
    @testset "Array type preservation" begin
        arr = [1, 2, 3]
        arr_copy = copy(arr)
        @test length(arr_copy) == 3
        @test arr_copy[1] == 1
        @test arr_copy[2] == 2
        @test arr_copy[3] == 3
        # Verify it's a copy, not the same reference
        arr_copy[1] = 100
        @test arr[1] == 1  # Original unchanged
    end

    @testset "Dict type preservation" begin
        dict = Dict("a" => 1, "b" => 2)
        dict_copy = copy(dict)
        @test length(dict_copy) == 2
        @test get(dict_copy, "a", 0) == 1
        @test get(dict_copy, "b", 0) == 2
        # Verify it's a copy
        dict_copy["a"] = 100
        @test get(dict, "a", 0) == 1  # Original unchanged
    end

    @testset "Set type preservation" begin
        set = Set([1, 2, 3])
        set_copy = copy(set)
        @test length(set_copy) == 3
        @test 1 in set_copy
        @test 2 in set_copy
        @test 3 in set_copy
    end

    @testset "Tuple type preservation" begin
        tup = (1, 2, 3)
        tup_copy = copy(tup)
        @test tup_copy == (1, 2, 3)
        @test length(tup_copy) == 3
        # Tuples are immutable, so copy returns identity
    end

    @testset "Empty collections" begin
        # Empty Array
        empty_arr = Int64[]
        @test length(copy(empty_arr)) == 0

        # Empty Dict
        empty_dict = Dict{String,Int64}()
        @test length(copy(empty_dict)) == 0

        # Empty Set
        empty_set = Set{Int64}()
        @test length(copy(empty_set)) == 0

        # Empty Tuple
        empty_tup = ()
        @test copy(empty_tup) == ()
    end

    @testset "Nested collections" begin
        # Array of Arrays
        nested_arr = [[1, 2], [3, 4]]
        nested_copy = copy(nested_arr)
        @test length(nested_copy) == 2
        @test nested_copy[1] == [1, 2]
        @test nested_copy[2] == [3, 4]

        # Dict with array values
        dict_with_arr = Dict("x" => [1, 2], "y" => [3, 4])
        dict_copy = copy(dict_with_arr)
        @test get(dict_copy, "x", Int64[])[1] == 1
        @test get(dict_copy, "y", Int64[])[1] == 3
    end
end

true
