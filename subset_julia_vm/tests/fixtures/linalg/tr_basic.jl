# Test tr (trace) function from LinearAlgebra

using Test
using LinearAlgebra

@testset "tr (trace) - sum of diagonal elements" begin

    # 2x2 identity matrix
    A = [1.0 0.0; 0.0 1.0]
    @assert tr(A) == 2.0 "trace of identity matrix"

    # 3x3 matrix
    B = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]
    @assert tr(B) == 15.0 "trace of 3x3 matrix"

    # Integer matrix
    C = [1 0 0; 0 2 0; 0 0 3]
    t = tr(C)
    @assert t == 6.0 "trace of diagonal matrix"

    @test (true)
end

true  # Test passed
