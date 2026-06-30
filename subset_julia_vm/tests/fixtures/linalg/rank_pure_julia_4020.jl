# Test LinearAlgebra.rank Pure Julia dispatch path (Issue #4020)

using LinearAlgebra
using Test

I3 = [1.0 0.0 0.0; 0.0 1.0 0.0; 0.0 0.0 1.0]
@test rank(I3) == 3
@test LinearAlgebra.rank(I3) == 3

R1 = [1.0 2.0; 2.0 4.0]
@test rank(R1) == 1

Z = [0.0 0.0; 0.0 0.0]
@test rank(Z) == 0

@test rank([1.0, 2.0, 3.0]) == 1
@test rank([0.0, 0.0, 0.0]) == 0
@test rank(2.0) == 1
@test rank(0.0) == 0

true
