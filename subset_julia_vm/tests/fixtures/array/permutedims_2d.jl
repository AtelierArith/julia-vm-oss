# Test permutedims for 2D matrix transpose

using Test

@testset "permutedims for 2D matrix transposes dimensions" begin
    A = zeros(2, 3)
    A[1,1] = 1
    A[1,2] = 2
    A[1,3] = 3
    A[2,1] = 4
    A[2,2] = 5
    A[2,3] = 6
    B = permutedims(A)
    # Check B[i,j] == A[j,i] (transposed)
    @test (size(B) == (3, 2) && B[1,1] == 1 && B[1,2] == 4 && B[2,1] == 2 && B[3,2] == 6)
end

true  # Test passed
