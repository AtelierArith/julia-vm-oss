# CartesianIndex array indexing test
# Tests A[CartesianIndex((i, j))] == A[i, j]

using Test

@testset "CartesianIndex array indexing: A[CartesianIndex((i,j))] == A[i,j]" begin

    # 2x2 matrix
    A = [1.0 3.0; 2.0 4.0]

    # Core requirement: CartesianIndex indexing must equal direct indexing
    # Test: A[CartesianIndex((i, j))] == A[i, j] for all index combinations
    I1 = CartesianIndex((1, 1))
    I2 = CartesianIndex((2, 1))
    I3 = CartesianIndex((1, 2))
    I4 = CartesianIndex((2, 2))

    test1 = A[I1] == A[1, 1]
    test2 = A[I2] == A[2, 1]
    test3 = A[I3] == A[1, 2]
    test4 = A[I4] == A[2, 2]

    # Test iteration with CartesianIndex indexing
    # For any array, iterating with CartesianIndices and summing via A[I]
    # should equal summing all elements
    B = [10.0 30.0; 20.0 40.0]
    sum_via_cartesian = 0.0
    for I in CartesianIndices((2, 2))
        sum_via_cartesian = sum_via_cartesian + B[I]
    end

    # Sum via direct indexing
    sum_direct = B[1,1] + B[2,1] + B[1,2] + B[2,2]
    test5 = sum_via_cartesian == sum_direct

    # All tests must pass
    @test (test1 && test2 && test3 && test4 && test5)
end

true  # Test passed
