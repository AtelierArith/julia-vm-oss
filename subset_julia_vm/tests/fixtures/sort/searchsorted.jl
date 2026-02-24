# Test searchsorted function
# Note: Uses searchsortedfirst/searchsortedlast to verify since first/last
# don't support Range types in SubsetJuliaVM

using Test

@testset "searchsorted: return range of indices for value in sorted array" begin

    # =============================================================================
    # searchsorted tests
    # =============================================================================

    arr1 = [1, 2, 4, 5, 5, 7]

    # Test single match: searchsorted returns 3:3
    # Verify by checking searchsortedfirst and searchsortedlast directly
    check1 = searchsortedfirst(arr1, 4) == 3 && searchsortedlast(arr1, 4) == 3

    # Test multiple matches: searchsorted returns 4:5
    check2 = searchsortedfirst(arr1, 5) == 4 && searchsortedlast(arr1, 5) == 5

    # Test no match, insert in middle: searchsorted returns 3:2 (empty)
    check3 = searchsortedfirst(arr1, 3) == 3 && searchsortedlast(arr1, 3) == 2

    # Test no match, insert at end: searchsorted returns 7:6 (empty)
    check4 = searchsortedfirst(arr1, 9) == 7 && searchsortedlast(arr1, 9) == 6

    # Test no match, insert at start: searchsorted returns 1:0 (empty)
    check5 = searchsortedfirst(arr1, 0) == 1 && searchsortedlast(arr1, 0) == 0

    # Test with floats
    arr2 = [1.0, 2.0, 3.0, 3.0, 3.0, 5.0]
    check6 = searchsortedfirst(arr2, 3.0) == 3 && searchsortedlast(arr2, 3.0) == 5

    # Test empty array
    arr3 = Int64[]
    check7 = searchsortedfirst(arr3, 5) == 1 && searchsortedlast(arr3, 5) == 0

    # Test single element array - found
    arr4 = [42]
    check8 = searchsortedfirst(arr4, 42) == 1 && searchsortedlast(arr4, 42) == 1

    # Test single element array - not found (less than)
    check9 = searchsortedfirst(arr4, 10) == 1 && searchsortedlast(arr4, 10) == 0

    # Test single element array - not found (greater than)
    check10 = searchsortedfirst(arr4, 100) == 2 && searchsortedlast(arr4, 100) == 1

    # Test isempty with searchsorted
    # searchsorted(arr, x) returns first:last, empty if first > last
    r3 = searchsorted(arr1, 3)  # Should be 3:2 (empty)
    r4 = searchsorted(arr1, 4)  # Should be 3:3 (not empty)
    check11 = isempty(r3) == true
    check12 = isempty(r4) == false

    # Test that searchsorted returns a range
    r_test = searchsorted(arr1, 5)  # Should be 4:5
    check13 = length(r_test) == 2  # Range 4:5 has length 2

    # All checks must pass
    @test (check1 && check2 && check3 && check4 && check5 && check6 && check7 && check8 && check9 && check10 && check11 && check12 && check13)
end

true  # Test passed
