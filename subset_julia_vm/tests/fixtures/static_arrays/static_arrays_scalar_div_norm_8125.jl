# Issue #8125: scalar division `A / c`, `norm` and `normalize` on static arrays.
# These work in upstream Julia but errored in sjulia — `/` had no method and the
# generic LinearAlgebra norm/normalize iterated a StaticArrayInline, which the VM
# does not support. The bundled StaticArrays package now provides index-based
# (non-iterating) implementations.
using StaticArrays
using LinearAlgebra

approx(a, b) = abs(a - b) < 1e-12

v2 = SVector(2.0, 4.0)
v3 = SVector(3.0, 4.0, 0.0)
v4 = SVector(2.0, 4.0, 6.0, 8.0)

d2 = v2 / 2.0
d3 = v3 / 2.0
d4 = v4 / 2.0
n3 = normalize(v3)

m2 = SMatrix{2,2}((2.0, 4.0, 6.0, 8.0)) / 2.0
m3 = SMatrix{3,3}((3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 21.0, 24.0, 27.0)) / 3.0

ok = # vector scalar division (n = 2, 3, 4 fast paths)
     approx(d2[1], 1.0) && approx(d2[2], 2.0) &&
     approx(d3[1], 1.5) && approx(d3[2], 2.0) && approx(d3[3], 0.0) &&
     approx(d4[4], 4.0) &&
     # Euclidean norm (index-based, no iteration)
     approx(norm(v3), 5.0) &&
     approx(norm(v2), sqrt(20.0)) &&
     # normalisation yields a unit vector along v3
     approx(n3[1], 0.6) && approx(n3[2], 0.8) && approx(n3[3], 0.0) &&
     approx(norm(n3), 1.0) &&
     # matrix scalar division (column-major preserved)
     approx(m2[1, 1], 1.0) && approx(m2[2, 2], 4.0) &&
     approx(m3[1, 1], 1.0) && approx(m3[3, 3], 9.0)

println(ok)
ok
