# Test permutedims for 3D array with permutation

using Test

@testset "permutedims for 3D array with permutation argument" begin
    A = zeros(2, 3, 4)
    for i in 1:2
        for j in 1:3
            for k in 1:4
                A[i, j, k] = i * 100 + j * 10 + k
            end
        end
    end
    B = permutedims(A, (3, 1, 2))
    # B[k,i,j] should equal A[i,j,k]
    # Check size and some specific values
    @test (size(B) == (4, 2, 3) && B[1,1,1] == 111 && B[2,1,1] == 112 && B[1,2,1] == 211 && B[1,1,2] == 121)
end

true  # Test passed
