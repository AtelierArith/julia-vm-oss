# Test cond() matrix condition number (Issue #647)

using LinearAlgebra
using Test

# Test cond on identity matrix (condition number = 1)
I2 = [1.0 0.0; 0.0 1.0]
@test cond(I2) == 1.0

# Test cond on well-conditioned matrix
A = [1.0 0.0; 0.0 2.0]
# Singular values are 2.0 and 1.0, so cond = 2.0 / 1.0 = 2.0
@test cond(A) == 2.0

# Test cond on another well-conditioned matrix
B = [3.0 0.0; 0.0 1.0]
# Singular values are 3.0 and 1.0, so cond = 3.0 / 1.0 = 3.0
@test cond(B) == 3.0

# Test cond on scaled identity (condition number = 1)
C = [5.0 0.0; 0.0 5.0]
@test cond(C) == 1.0

# Test cond on singular/nearly-singular matrix (condition number is very large)
D = [1.0 2.0; 2.0 4.0]
# Julia returns a very large number (not exactly Inf due to numerical precision)
# Our implementation returns Inf, both are correct behaviors
cond_D = cond(D)
@test cond_D > 1e10 || isinf(cond_D)

# Test cond on non-square matrix
E = [1.0 2.0 3.0; 4.0 5.0 6.0]
c = cond(E)
@test c > 0  # Should be a positive number

# Return true to indicate success
true
