using Test

@testset "Array sort operations" begin
    @testset "sort (non-mutating)" begin
        # Basic sort
        arr = [3, 1, 4, 1, 5, 9, 2, 6]
        sorted = sort(arr)
        @test sorted == [1, 1, 2, 3, 4, 5, 6, 9]
        @test arr == [3, 1, 4, 1, 5, 9, 2, 6]  # original unchanged

        # Already sorted
        @test sort([1, 2, 3, 4, 5]) == [1, 2, 3, 4, 5]

        # Reverse sorted
        @test sort([5, 4, 3, 2, 1]) == [1, 2, 3, 4, 5]

        # Single element
        @test sort([42]) == [42]

        # Float sort
        arr_f = [3.1, 1.4, 2.7]
        @test sort(arr_f) == [1.4, 2.7, 3.1]
    end

    @testset "sort! (mutating)" begin
        # Basic in-place sort
        arr = [3, 1, 4, 1, 5]
        sort!(arr)
        @test arr == [1, 1, 3, 4, 5]

        # Returns the sorted array (same reference)
        arr2 = [5, 3, 1]
        result = sort!(arr2)
        @test result == [1, 3, 5]
        @test result === arr2
    end

    @testset "sort with rev=true" begin
        arr = [3, 1, 4, 1, 5]
        @test sort(arr, rev=true) == [5, 4, 3, 1, 1]
    end

    @testset "unique" begin
        # Remove duplicates preserving order
        @test unique([1, 2, 1, 3, 2, 4]) == [1, 2, 3, 4]

        # All unique
        @test unique([1, 2, 3]) == [1, 2, 3]

        # All duplicates
        @test unique([5, 5, 5]) == [5]

        # Single element
        @test unique([42]) == [42]
    end
end

true
