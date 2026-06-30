using Test
using LinearAlgebra

@testset "vector-matrix multiplication dispatches before MatMul fallback (Issue #4020)" begin
    v = [1.0, 2.0]
    B = [3.0 4.0]

    default_values = v * B
    @test size(default_values, 1) == 2
    @test size(default_values, 2) == 2
    @test default_values[1, 1] == 3.0
    @test default_values[1, 2] == 4.0
    @test default_values[2, 1] == 6.0
    @test default_values[2, 2] == 8.0

    Base.:*(x::Vector{Int64}, A::Matrix{Int64}) = [4020]

    override_values = [1, 2] * [1 2; 3 4]
    @test length(override_values) == 1
    @test override_values[1] == 4020
end

true
