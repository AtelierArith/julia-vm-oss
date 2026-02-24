# Test rank() matrix rank computation (Issue #645)

using LinearAlgebra
using Test

# Test rank on full rank matrix (identity)
I2 = [1.0 0.0; 0.0 1.0]
@test rank(I2) == 2

# Test rank on full rank non-square matrix
A = [1.0 2.0 3.0; 4.0 5.0 6.0]
@test rank(A) == 2

# Test rank on rank-deficient matrix
# B has rank 1 (second row is 2x first row)
B = [1.0 2.0; 2.0 4.0]
@test rank(B) == 1

# Test rank on zero matrix
Z = [0.0 0.0; 0.0 0.0]
@test rank(Z) == 0

# Test rank on single row matrix (rank 1)
R = [1.0 2.0 3.0]
# Need to reshape to 2D for rank
R2 = reshape(R, 1, 3)
@test rank(R2) == 1

# Return true to indicate success
true
