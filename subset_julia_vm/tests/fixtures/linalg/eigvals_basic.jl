# Test eigvals() eigenvalue decomposition (Issue #641)

using LinearAlgebra
using Test

# Test eigvals on 2x2 symmetric matrix
# For symmetric matrices, eigenvalues are real
A = [2.0 1.0; 1.0 2.0]
vals = eigvals(A)

# Check we got 2 eigenvalues
@test length(vals) == 2

# For 2x2 symmetric matrix [[a,b],[b,a]], eigenvalues are a+b and a-b
# So eigenvalues of [[2,1],[1,2]] are 3 and 1
# Note: order is not guaranteed, so check both values exist
v1 = vals[1]
v2 = vals[2]

# Get real parts (eigenvalues should be real for symmetric matrix)
r1 = real(v1)
r2 = real(v2)

# Check imaginary parts are essentially zero
@test abs(imag(v1)) < 1e-10
@test abs(imag(v2)) < 1e-10

# Check the eigenvalues are approximately 1 and 3 (in either order)
sum_eig = r1 + r2
prod_eig = r1 * r2

# Sum of eigenvalues = trace = 4
@test isapprox(sum_eig, 4.0, atol=1e-10)

# Product of eigenvalues = determinant = 2*2 - 1*1 = 3
@test isapprox(prod_eig, 3.0, atol=1e-10)

# Return true to indicate success
true
