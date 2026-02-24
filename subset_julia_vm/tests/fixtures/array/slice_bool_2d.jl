# Test slicing 2D Bool arrays preserves element type
# Issue #1565: Array slicing only handles F64/I64, other types become 0.0

using Test

@testset "2D Bool array slicing" begin
    # Create a 2D Bool array (3x3 matrix)
    arr = [true false true; false true false; true true false]

    # Test column slice: should return 1D vector
    col1 = arr[:, 1]
    @test length(col1) == 3
    @test ndims(col1) == 1
    @test col1[1] == true
    @test col1[2] == false
    @test col1[3] == true
    @test eltype(col1) == Bool

    # Test row slice: should return 1D vector
    row1 = arr[1, :]
    @test length(row1) == 3
    @test ndims(row1) == 1
    @test row1[1] == true
    @test row1[2] == false
    @test row1[3] == true
    @test eltype(row1) == Bool

    # Test full slice: should return 2D matrix
    full = arr[:, :]
    @test size(full) == (3, 3)
    @test ndims(full) == 2
    @test full[1, 1] == true
    @test full[2, 2] == true
    @test eltype(full) == Bool
end

true
