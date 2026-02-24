# Test issetequal() function
# issetequal(a, b) - check if two collections have the same elements (as sets)

using Test

@testset "issetequal - check if two collections have the same elements as sets" begin

    result = 0.0

    # Test 1: Sets with same elements
    a = Set([1, 2, 3])
    b = Set([3, 2, 1])
    if issetequal(a, b)
        result = result + 1.0
    end

    # Test 2: Sets with different elements
    c = Set([1, 2, 3])
    d = Set([1, 2, 4])
    if !issetequal(c, d)
        result = result + 1.0
    end

    # Test 3: Sets with different sizes
    e = Set([1, 2, 3])
    f = Set([1, 2])
    if !issetequal(e, f)
        result = result + 1.0
    end

    # Test 4: Empty sets
    g = Set([])
    h = Set([])
    if issetequal(g, h)
        result = result + 1.0
    end

    # Test 5: Arrays with same elements (different order)
    arr1 = [1.0, 2.0, 3.0]
    arr2 = [3.0, 1.0, 2.0]
    if issetequal(arr1, arr2)
        result = result + 1.0
    end

    # Test 6: Arrays with duplicates (treated as sets)
    arr3 = [1.0, 1.0, 2.0, 2.0, 3.0]
    arr4 = [1.0, 2.0, 3.0]
    if issetequal(arr3, arr4)
        result = result + 1.0
    end

    # Test 7: Arrays with different elements
    arr5 = [1.0, 2.0, 3.0]
    arr6 = [1.0, 2.0, 4.0]
    if !issetequal(arr5, arr6)
        result = result + 1.0
    end

    @test (result) == 7.0
end

true  # Test passed
