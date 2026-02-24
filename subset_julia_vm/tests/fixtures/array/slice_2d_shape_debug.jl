# Debug test for 2D slice shape
# Check the actual shape returned by slicing

using Test

@testset "2D slice shape debug" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2x3 matrix

    # Column slice
    col1 = A[:, 1]
    col_shape = size(col1)
    col_ndims = ndims(col1)
    col_len = length(col1)

    # Check what we actually get
    @test col_len == 2

    # In Julia: size(A[:, 1]) should be (2,) not (2, 1)
    # ndims(A[:, 1]) should be 1 not 2
    @test col_ndims == 1

    # Row slice
    row1 = A[1, :]
    row_shape = size(row1)
    row_ndims = ndims(row1)
    row_len = length(row1)

    @test row_len == 3
    @test row_ndims == 1
end

true
