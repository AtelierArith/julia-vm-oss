using StaticArrays
using LinearAlgebra

# Phase 5 small static linear algebra (Issue #7461): transpose, trace, diagonal,
# determinant (1×1/2×2/3×3), inverse (1×1/2×2), dot and norm, all returning
# upstream-compatible values and static results where upstream does. Type checks
# use `isa` so the assertions hold under both sjulia and upstream Julia (whose
# SMatrix carries an extra length parameter). `adjoint` is deferred for the
# real-valued MVP (adjoint == transpose; see linalg.jl).

approx(a, b) = abs(a - b) < 1e-12
# `diag` returns an SVector, but the generic `LinearAlgebra.diag` it overrides is
# inferred to return a plain `Vector`, so a *direct* `diag(A)[i]` mis-compiles to
# a typed array load in sjulia (the package override is invisible to compile-time
# inference). Reading the element through this barrier forces a generic index and
# works under both runtimes (tracked as a known inference gap in Issue #8132).
elt(x, i) = x[i]

A = @SMatrix [1.0 2.0; 3.0 4.0]
A3 = @SMatrix [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 10.0]
v = SVector(1.0, 2.0, 3.0)
w = SVector(4.0, 5.0, 6.0)

# transpose: [1 2; 3 4]^T = [1 3; 2 4]; column-major tuple (1, 2, 3, 4).
At = transpose(A)
trans_ok = Tuple(At) == (1.0, 2.0, 3.0, 4.0) && At isa SMatrix{2,2,Float64} &&
           At[1, 2] == 3.0 && At[2, 1] == 2.0 &&
           Tuple(transpose(A3)) == (1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0)

# trace / diagonal (diag elements read via the `elt` barrier — see above).
dg = diag(A)
dg3 = diag(A3)
diag_ok = tr(A) == 5.0 && tr(A3) == 16.0 &&
          dg isa SVector{2,Float64} && elt(dg, 1) == 1.0 && elt(dg, 2) == 4.0 &&
          dg3 isa SVector{3,Float64} &&
          elt(dg3, 1) == 1.0 && elt(dg3, 2) == 5.0 && elt(dg3, 3) == 10.0

# determinant (closed forms).
det_ok = approx(det(A), -2.0) && approx(det(A3), -3.0) &&
         approx(det(SMatrix{1,1}(5.0)), 5.0)

# inverse (1×1 / 2×2). B = [4 7; 2 6] ⇒ inv(B) = [0.6 -0.7; -0.2 0.4].
B = @SMatrix [4.0 7.0; 2.0 6.0]
Bi = inv(B)
inv_ok = Bi isa SMatrix{2,2,Float64} &&
         approx(Bi[1, 1], 0.6) && approx(Bi[2, 1], -0.2) &&
         approx(Bi[1, 2], -0.7) && approx(Bi[2, 2], 0.4) &&
         approx(Tuple(inv(SMatrix{1,1}(2.0)))[1], 0.5)

# inv(B) * B ≈ I (sanity for the 2×2 closed form).
ident = Bi * B
id_ok = approx(ident[1, 1], 1.0) && approx(ident[2, 2], 1.0) &&
        approx(ident[1, 2], 0.0) && approx(ident[2, 1], 0.0)

# dot / norm.
prod_ok = approx(dot(v, w), 32.0) && approx(norm(SVector(3.0, 4.0)), 5.0)

ok = trans_ok && diag_ok && det_ok && inv_ok && id_ok && prod_ok

println((trans_ok, diag_ok, det_ok, inv_ok, id_ok, prod_ok, ok))
ok
