# Test matrix multiplication

using Test
using LinearAlgebra

@testset "matrix multiplication: A * B" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    B = [7.0 8.0; 9.0 10.0; 11.0 12.0]
    C = A * B

    result = true
    result = result && size(C, 1) == 2
    result = result && size(C, 2) == 2
    result = result && C[1, 1] == 58.0
    result = result && C[1, 2] == 64.0
    result = result && C[2, 1] == 139.0
    result = result && C[2, 2] == 154.0

    @test result
end

true  # Test passed
