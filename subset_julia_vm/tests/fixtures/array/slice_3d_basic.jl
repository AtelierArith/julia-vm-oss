# Test 3D array slicing
# Issue #1564: N-D array slicing (N > 2) returns empty array

using Test

@testset "3D array slicing" begin
    # Create a 3x4x2 array
    # Using manual construction since reshape might have issues
    A = zeros(3, 4, 2)

    # Fill with sequential values for testing
    val = 1.0
    for k in 1:2
        for j in 1:4
            for i in 1:3
                A[i, j, k] = val
                val += 1.0
            end
        end
    end

    # Test basic indexing
    @test A[1, 1, 1] == 1.0
    @test A[3, 4, 2] == 24.0

    # Test slice with scalar first dimension: A[1, :, :]
    # Should return 4x2 matrix
    slice1 = A[1, :, :]
    @test ndims(slice1) == 2
    @test size(slice1) == (4, 2)
    @test slice1[1, 1] == 1.0   # A[1,1,1]
    @test slice1[2, 1] == 4.0   # A[1,2,1]

    # Test slice with scalar second dimension: A[:, 2, :]
    # Should return 3x2 matrix
    slice2 = A[:, 2, :]
    @test ndims(slice2) == 2
    @test size(slice2) == (3, 2)
    @test slice2[1, 1] == 4.0   # A[1,2,1]
    @test slice2[2, 1] == 5.0   # A[2,2,1]

    # Test slice with scalar third dimension: A[:, :, 1]
    # Should return 3x4 matrix
    slice3 = A[:, :, 1]
    @test ndims(slice3) == 2
    @test size(slice3) == (3, 4)
    @test slice3[1, 1] == 1.0   # A[1,1,1]
    @test slice3[3, 4] == 12.0  # A[3,4,1]

    # Test full slice: A[:, :, :]
    # Should return 3x4x2 array
    full = A[:, :, :]
    @test ndims(full) == 3
    @test size(full) == (3, 4, 2)
    @test full[1, 1, 1] == 1.0
end

true
