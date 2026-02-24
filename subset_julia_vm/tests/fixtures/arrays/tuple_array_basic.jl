# Test basic tuple array creation and indexing

using Test

@testset "Tuple array creation and indexing" begin
    arr = [(1, 2.0), (3, 4.0), (5, 6.0)]

    # Test indexing
    t1 = arr[1]
    t2 = arr[2]
    t3 = arr[3]

    # Sum of first elements
    result = t1[1] + t2[1] + t3[1]  # 1 + 3 + 5 = 9
    @test (result) == 9.0
end

true  # Test passed
