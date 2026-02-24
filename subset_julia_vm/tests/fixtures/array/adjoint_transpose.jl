# Test Pure Julia adjoint and transpose for arrays

using Test

@testset "Array transpose and adjoint (Pure Julia)" begin
    # 1D vector transpose -> row vector
    v = [1.0, 2.0, 3.0]
    vt = transpose(v)
    @test size(vt) == (1, 3)
    @test vt[1, 1] == 1.0
    @test vt[1, 2] == 2.0
    @test vt[1, 3] == 3.0

    # 2D matrix transpose
    A = [1.0 2.0; 3.0 4.0]
    At = transpose(A)
    @test size(At) == (2, 2)
    @test At[1, 1] == 1.0
    @test At[1, 2] == 3.0
    @test At[2, 1] == 2.0
    @test At[2, 2] == 4.0

    # adjoint for real matrix (same as transpose)
    Ad = adjoint(A)
    @test size(Ad) == (2, 2)
    @test Ad[1, 1] == 1.0
    @test Ad[1, 2] == 3.0
    @test Ad[2, 1] == 2.0
    @test Ad[2, 2] == 4.0

    # 1D vector adjoint -> row vector (conjugated)
    va = adjoint(v)
    @test size(va) == (1, 3)
    @test va[1, 1] == 1.0
    @test va[1, 2] == 2.0
    @test va[1, 3] == 3.0
end

true
