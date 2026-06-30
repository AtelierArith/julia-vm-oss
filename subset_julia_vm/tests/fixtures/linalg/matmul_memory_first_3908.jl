using Test
using LinearAlgebra

@testset "matmul results remain Julia-visible after Memory-first materialization (Issue #3908)" begin
    A = [1.0 2.0; 3.0 4.0]
    B = [5.0 6.0; 7.0 8.0]
    C = A * B

    @test C isa Matrix{Float64}
    @test size(C) == (2, 2)
    @test C[1, 1] == 19.0
    @test C[2, 1] == 43.0
    @test C[1, 2] == 22.0
    @test C[2, 2] == 50.0

    v = [1.0, 2.0, 3.0]
    scaled = 2.0 * v

    @test scaled isa Vector{Float64}
    @test size(scaled) == (3,)
    @test scaled[1] == 2.0
    @test scaled[2] == 4.0
    @test scaled[3] == 6.0
end

true
