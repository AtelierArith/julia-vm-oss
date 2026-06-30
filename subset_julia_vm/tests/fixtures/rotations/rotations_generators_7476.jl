# Phase 5 (Issue #7476): rotation generators (skew-symmetric matrices)
# Angle2dGenerator / RotationVecGenerator, plus skew and isrotationgenerator.
using Rotations
using StaticArrays

approx(a, b) = all(abs.(a .- b) .< 1e-12)

# 2-D generator [0 -v; v 0]
g2 = Angle2dGenerator(0.5)
# 3-D generator [0 -z y; z 0 -x; -y x 0]
g3 = RotationVecGenerator(1.0, 2.0, 3.0)
# skew-symmetric (cross-product) matrix of a vector
s = Rotations.skew(SVector(1.0, 2.0, 3.0))

ok = g2 isa Angle2dGenerator{Float64} &&
     g3 isa RotationVecGenerator{Float64} &&
     g2 isa RotationGenerator &&
     # column-major tuples match the skew-symmetric layout
     approx(Tuple(g2), (0.0, 0.5, -0.5, 0.0)) &&
     approx(Tuple(g3), (0.0, 3.0, -2.0, -3.0, 0.0, 1.0, 2.0, -1.0, 0.0)) &&
     # 2-D indexing
     g2[2, 1] == 0.5 && g2[1, 2] == -0.5 &&
     # skew-symmetric algebra stays within the generator type
     approx(Tuple(g2 + g2), (0.0, 1.0, -1.0, 0.0)) &&
     approx(Tuple(2.0 * g2), (0.0, 1.0, -1.0, 0.0)) &&
     approx(Tuple(g3 - g3), (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)) &&
     approx(Tuple(-g3), (0.0, -3.0, 2.0, 3.0, 0.0, -1.0, -2.0, 1.0, 0.0)) &&
     # params
     approx(Tuple(Rotations.params(g2)), (0.5,)) &&
     approx(Tuple(Rotations.params(g3)), (1.0, 2.0, 3.0)) &&
     # skew builds the expected matrix and is a generator
     approx(Tuple(s), (0.0, 3.0, -2.0, -3.0, 0.0, 1.0, 2.0, -1.0, 0.0)) &&
     isrotationgenerator(s) &&
     isrotationgenerator(g3) &&
     # transpose negates a skew-symmetric matrix
     approx(Tuple(transpose(g3)), Tuple(-g3)) &&
     size(g3) == (3, 3)

println(ok)
ok
