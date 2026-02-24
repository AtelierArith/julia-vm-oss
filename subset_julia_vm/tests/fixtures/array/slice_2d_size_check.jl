# Explicit size check for 2D slicing
# This test will fail if slicing returns wrong shape

using Test

@testset "2D slice size check" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2x3 matrix

    # Check original matrix
    @test size(A) == (2, 3)
    @test ndims(A) == 2

    # Column slice - should be 1D
    col1 = A[:, 1]
    s = size(col1)

    # In Julia, size of 1D array returns (n,) which is a 1-tuple
    # length(size(col1)) should be 1
    @test length(s) == 1
    @test s[1] == 2  # 2 elements

    # If this fails, the slice returned a 2D array
    # with shape (2, 1) instead of (2,)
end

true
