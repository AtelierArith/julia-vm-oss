# Reproduction of iOS eigen bug
# Test case exactly as in iOS sample

using LinearAlgebra
using Test

A = rand(3, 3)
F = eigen(A)
v1 = F.vectors[:, 1]
λ1 = F.values[1]

@test A * v1 ≈ λ1 * v1

true
