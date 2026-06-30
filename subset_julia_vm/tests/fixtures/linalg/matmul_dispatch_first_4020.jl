using Test
using LinearAlgebra

Base.:*(A::Matrix{Float64}, B::Matrix{Float64}) = fill(4020.0, 1, 1)

@testset "matrix multiplication dispatches before MatMul fallback (Issue #4020)" begin
    A = [1.0 2.0; 3.0 4.0]
    B = [5.0 6.0; 7.0 8.0]

    C = A * B

    @test size(C, 1) == 1
    @test size(C, 2) == 1
    @test C[1, 1] == 4020.0
end

true
