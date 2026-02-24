# Set comprehension tests

using Test

@testset "Set comprehension: Set(x for x in iter [if cond])" begin

    # Basic Set comprehension from array
    arr = [1, 2, 3, 2, 1]  # has duplicates
    s = Set(x for x in arr)
    # Set should have 3 unique elements: 1, 2, 3

    # Set comprehension from range
    s2 = Set(i for i in 1:5)
    # Set should have 5 elements: 1, 2, 3, 4, 5

    # Set comprehension with filter
    s3 = Set(x for x in 1:10 if x > 5)
    # Set should have 5 elements: 6, 7, 8, 9, 10

    # Set constructor from array
    arr2 = [10, 20, 30, 20, 10]
    s4 = Set(arr2)
    # Set should have 3 unique elements

    # Verify: 3 + 5 + 5 + 3 = 16
    result = length(s) + length(s2) + length(s3) + length(s4)
    @test (result) == 16.0
end

true  # Test passed
