# Test ndims function - returns number of dimensions
# Issue #587

using Test

@testset "ndims: return number of dimensions (Issue #587)" begin

    vec1d = [1, 2, 3]
    mat2d = [1 2; 3 4]
    arr3d = zeros(2, 3, 4)

    test1 = ndims(vec1d) == 1
    test2 = ndims(mat2d) == 2
    test3 = ndims(arr3d) == 3

    @test (test1 && test2 && test3)
end

true  # Test passed
