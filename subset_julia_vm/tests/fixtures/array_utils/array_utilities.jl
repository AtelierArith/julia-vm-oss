# Array utility functions test - Pure Julia implementations
# Tests for reverse, reverse!, copy, fill!, circshift, circshift!

using Test

@testset "Array utility functions" begin
    @testset "reverse (non-mutating)" begin
        # Basic reverse
        arr = [1, 2, 3, 4, 5]
        rev = reverse(arr)
        @test rev == [5, 4, 3, 2, 1]
        @test arr == [1, 2, 3, 4, 5]  # Original unchanged

        # Single element
        @test reverse([42]) == [42]

        # Float array
        arr_f = [1.0, 2.0, 3.0]
        @test reverse(arr_f) == [3.0, 2.0, 1.0]
    end

    @testset "reverse! (mutating)" begin
        # Basic in-place reverse
        arr = [1, 2, 3, 4, 5]
        reverse!(arr)
        @test arr == [5, 4, 3, 2, 1]

        # Even length array
        arr2 = [1, 2, 3, 4]
        reverse!(arr2)
        @test arr2 == [4, 3, 2, 1]

        # Single element
        arr3 = [42]
        reverse!(arr3)
        @test arr3 == [42]

        # reverse! with range
        arr4 = [1, 2, 3, 4, 5]
        reverse!(arr4, 2, 4)  # Reverse indices 2 to 4
        @test arr4 == [1, 4, 3, 2, 5]
    end

    @testset "copy (non-mutating)" begin
        # Basic copy
        arr = [1, 2, 3]
        arr_copy = copy(arr)
        @test arr_copy == arr

        # Verify it's a new array (modification doesn't affect original)
        arr_copy[1] = 100
        @test arr[1] == 1
        @test arr_copy[1] == 100

        # Float array copy
        arr_f = [1.5, 2.5, 3.5]
        @test copy(arr_f) == arr_f
    end

    @testset "fill! (mutating)" begin
        # Fill with integer
        arr = [1, 2, 3, 4, 5]
        fill!(arr, 0)
        @test arr == [0, 0, 0, 0, 0]

        # Fill with float
        arr_f = [1.0, 2.0, 3.0]
        fill!(arr_f, 3.14)
        @test arr_f == [3.14, 3.14, 3.14]

        # Fill returns the modified array
        arr2 = [1, 2, 3]
        result = fill!(arr2, 42)
        @test result == [42, 42, 42]
        @test result === arr2  # Same array reference
    end

    @testset "circshift (non-mutating)" begin
        # Shift right by positive k
        arr = [1, 2, 3, 4, 5]
        @test circshift(arr, 1) == [5, 1, 2, 3, 4]
        @test circshift(arr, 2) == [4, 5, 1, 2, 3]
        @test arr == [1, 2, 3, 4, 5]  # Original unchanged

        # Shift left by negative k
        @test circshift(arr, -1) == [2, 3, 4, 5, 1]
        @test circshift(arr, -2) == [3, 4, 5, 1, 2]

        # No shift
        @test circshift(arr, 0) == [1, 2, 3, 4, 5]

        # Full cycle (shift by length)
        @test circshift(arr, 5) == [1, 2, 3, 4, 5]

        # Shift more than length (wraps around)
        @test circshift(arr, 7) == [4, 5, 1, 2, 3]  # Same as shift by 2
    end

    @testset "circshift! (mutating)" begin
        # Shift right
        arr = [1, 2, 3, 4, 5]
        circshift!(arr, 1)
        @test arr == [5, 1, 2, 3, 4]

        # Shift left
        arr2 = [1, 2, 3, 4, 5]
        circshift!(arr2, -2)
        @test arr2 == [3, 4, 5, 1, 2]

        # Returns modified array
        arr3 = [1, 2, 3]
        result = circshift!(arr3, 1)
        @test result === arr3
    end
end

true
