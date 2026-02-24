# Test matrix literal storage order (column-major)
# Issue #583: Matrix literal [1 2 3; 4 5 6] should use column-major order

using Test

@testset "Matrix literal [1 2; 3 4] uses column-major storage order" begin

    A = [1 2 3; 4 5 6]

    # Test element access by row,column indexing
    test1 = A[1,1] == 1 && A[1,2] == 2 && A[1,3] == 3
    test2 = A[2,1] == 4 && A[2,2] == 5 && A[2,3] == 6

    # Test linear indexing (column-major order: A[1]=1, A[2]=4, A[3]=2, A[4]=5, A[5]=3, A[6]=6)
    test3 = A[1] == 1 && A[2] == 4 && A[3] == 2
    test4 = A[4] == 5 && A[5] == 3 && A[6] == 6

    @test (test1 && test2 && test3 && test4)
end

true  # Test passed
