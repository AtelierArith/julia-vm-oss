# Test 2D array slicing dimension handling
# Issue #1562: A[:, i] should return 1D vector, not 2D array

using Test

@testset "2D slice dimension handling" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2x3 matrix

    # Test column slice: A[:, 1] should return 1D vector
    col1 = A[:, 1]
    @test length(col1) == 2
    @test ndims(col1) == 1  # Should be 1D, not 2D
    @test col1[1] == 1.0
    @test col1[2] == 4.0

    # Test row slice: A[1, :] should return 1D vector
    row1 = A[1, :]
    @test length(row1) == 3
    @test ndims(row1) == 1  # Should be 1D, not 2D
    @test row1[1] == 1.0
    @test row1[2] == 2.0
    @test row1[3] == 3.0

    # Test full slice: A[:, :] should return 2D matrix
    full = A[:, :]
    @test size(full) == (2, 3)
    @test ndims(full) == 2

    # Test range slice: A[:, 1:2] should return 2D matrix
    cols12 = A[:, 1:2]
    @test size(cols12) == (2, 2)
    @test ndims(cols12) == 2
end

true
