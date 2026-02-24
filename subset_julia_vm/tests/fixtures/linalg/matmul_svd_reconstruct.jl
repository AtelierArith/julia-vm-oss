# Test matrix multiplication and SVD reconstruction

using LinearAlgebra
using Test

@testset "matrix multiplication: backslash and SVD reconstruction" begin
    A = rand(3, 3)
    y = rand(3)
    x = A \ y
    ok = isapprox(y, A * x)

    A = rand(3, 4)
    F = svd(A)
    U = F.U
    S = F.S
    Vt = F.Vt

    ok = ok && isapprox(A, U * Diagonal(S) * Vt)

    @test ok
end

true  # Test passed
