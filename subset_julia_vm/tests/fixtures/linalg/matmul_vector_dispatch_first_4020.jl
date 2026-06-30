using Test
using LinearAlgebra

Base.:*(A::Matrix{Float64}, x::Vector{Float64}) = fill(4020.0, 1)
Base.:*(A::Matrix{Float64}, x::Vector{Complex{Float64}}) = [Complex{Float64}(4020.0, 1.0)]

@testset "matrix-vector multiplication dispatches before MatMul fallback (Issue #4020)" begin
    A = [1.0 2.0; 3.0 4.0]

    real_values = A * [5.0, 6.0]
    @test length(real_values) == 1
    @test real_values[1] == 4020.0

    complex_values = A * [Complex{Float64}(1.0, 1.0), Complex{Float64}(2.0, -1.0)]
    @test length(complex_values) == 1
    @test complex_values[1].re == 4020.0
    @test complex_values[1].im == 1.0
end

true
