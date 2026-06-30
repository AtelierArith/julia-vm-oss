# Phase 1 (Issue #7472): the bundled Rotations + Quaternions packages resolve
# through the @packages loader.
using Quaternions
using Rotations
using StaticArrays

q = Quaternion(1.0, 2.0, 3.0, 4.0)
r = RotMatrix(0.0)

ok = real(q) == 1.0 &&
     imag_part(q) == (2.0, 3.0, 4.0) &&
     r isa RotMatrix &&
     Tuple(r) == (1.0, 0.0, 0.0, 1.0)

println(ok)
ok
