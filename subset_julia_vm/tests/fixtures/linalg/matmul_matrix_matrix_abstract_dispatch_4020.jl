using Test
using LinearAlgebra

Base.:*(A::AbstractMatrix, x::AbstractVector) = fill(-4020.0, size(A, 1))

@testset "Matrix * Matrix does not dispatch as Matrix * Vector (Issue #4020)" begin
    @test !(Matrix{Float64} <: AbstractVector)
    @test Matrix{Float64} <: AbstractMatrix

    A = [1.0 2.0 3.0; 2.0 4.0 6.0; 3.0 6.0 9.0]
    B = zeros(3, 2)
    C = A * B

    @test size(C) == (3, 2)
    @test C[1, 1] == 0.0
    @test C[3, 2] == 0.0
end

true
