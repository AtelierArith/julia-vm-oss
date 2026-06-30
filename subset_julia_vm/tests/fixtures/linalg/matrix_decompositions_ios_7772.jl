# Matrix decompositions with LinearAlgebra — exact iOS app sample (Issue #7772).
#
# Regression guard: the stdlib `LinearAlgebra.lu`/`det`/`inv`/`svd`/`eigen`/...
# forwarders reach nalgebra kernels through a private compiler bridge. Dispatch
# regressions must not route those calls back into the forwarders and overflow
# the VM stack.
using LinearAlgebra
using Test

# Solve a linear system A x = y
A = rand(3, 3)
y = rand(3)
x = A \ y
@test y ≈ A * x

# Eigenvalue decomposition: A v = λ v
F = eigen(A)
v1 = F.vectors[:, 1]
λ1 = F.values[1]
@test A * v1 ≈ λ1 * v1

# Singular value decomposition: B = U Σ V'
B = rand(3, 4)
G = svd(B)
U, S, V = G
@test V' ≈ G.Vt
@test B ≈ U * Diagonal(S) * V'

true
