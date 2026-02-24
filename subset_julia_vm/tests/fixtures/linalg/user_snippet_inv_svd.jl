# User-reported inv/svd regression snippet
using LinearAlgebra
using Test

A = rand(3, 3)
y = rand(3)
x = A \ y
@test y ≈ A * x

A = rand(3, 4)
F = svd(A)
U, S, V = F
@test V' ≈ F.Vt
@test A ≈ U * Diagonal(S) * V'

true
