# Phase 4 (Issue #7475): rotation_between(u, v) — the rotation aligning u with v
# along the shortest geodesic. 2-D returns an Angle2d, 3-D returns a QuatRotation.
using Rotations
using StaticArrays
using LinearAlgebra

approx(a, b) = all(abs.(a .- b) .< 1e-10)
unit(x) = x / norm(x)

# 2-D: +x onto +y is a +90° rotation
u2 = SVector(1.0, 0.0)
v2 = SVector(0.0, 1.0)
r2 = rotation_between(u2, v2)

# 3-D: arbitrary pair; the result must map û onto v̂
u3 = SVector(1.0, 2.0, 3.0)
v3 = SVector(-2.0, 1.0, 0.5)
r3 = rotation_between(u3, v3)

# 3-D: an axis-aligned pair (+x onto +z), result equals RotY(-90°) action
ux = SVector(1.0, 0.0, 0.0)
uz = SVector(0.0, 0.0, 1.0)
rxz = rotation_between(ux, uz)

ok = r2 isa Angle2d{Float64} &&
     r3 isa QuatRotation{Float64} &&
     # 2-D angle is +π/2 and the rotation maps u2 onto v2
     abs(rotation_angle(r2) - (π / 2)) < 1e-10 &&
     approx(Tuple(r2 * u2), Tuple(v2)) &&
     # 3-D rotation maps the normalised u onto the normalised v
     approx(Tuple(r3 * unit(u3)), Tuple(unit(v3))) &&
     # it is a rotation (preserves length)
     abs(norm(r3 * u3) - norm(u3)) < 1e-10 &&
     # axis-aligned case maps x̂ onto ẑ
     approx(Tuple(rxz * ux), Tuple(uz)) &&
     # rotating u onto itself is (numerically) the identity
     approx(Tuple(rotation_between(u3, u3) * u3), Tuple(u3))

println(ok)
ok
