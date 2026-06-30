# Phase 2 (Issue #7473): 2-D rotations — RotMatrix and Angle2d.
# Deterministic subset of extern/Rotations.jl/test/2d.jl (no random loops).
using Rotations
using StaticArrays

approx(a, b) = all(abs.(a .- b) .< 1e-12)

theta = pi / 6
r = RotMatrix(theta)
a = Angle2d(theta)
v = SVector(2.0, -1.0)
s, c = sincos(theta)

ok = r isa RotMatrix{2,Float64} &&
     # construction & entries (column-major [c -s; s c])
     approx(Tuple(r), (c, s, -s, c)) &&
     r[1, 1] == c && r[2, 1] == s && r[1, 2] == -s && r[2, 2] == c &&
     # typed-eltype construction preserves the element type
     RotMatrix{2,Float32}(theta) isa RotMatrix{2,Float32} &&
     # flat tuple constructor (column-major)
     Tuple(RotMatrix((1.0, 0.0, 0.0, 1.0))) == (1.0, 0.0, 0.0, 1.0) &&
     # vector rotation matches the closed form, and agrees between RotMatrix/Angle2d
     approx(Tuple(r * v), (c * v[1] - s * v[2], s * v[1] + c * v[2])) &&
     approx(Tuple(a * v), Tuple(r * v)) &&
     # inverse and composition
     approx(Tuple(inv(r) * r), (1.0, 0.0, 0.0, 1.0)) &&
     abs(rotation_angle(RotMatrix(0.5) * RotMatrix(0.3)) - 0.8) < 1e-12 &&
     abs(rotation_angle(Angle2d(0.5) * Angle2d(0.3)) - 0.8) < 1e-12 &&
     abs(rotation_angle(RotMatrix(0.5) / RotMatrix(0.3)) - 0.2) < 1e-12 &&
     abs(rotation_angle(RotMatrix(0.5) \ RotMatrix(0.3)) + 0.2) < 1e-12 &&
     # rotation_angle round-trips, one is identity, isrotation holds
     abs(rotation_angle(r) - theta) < 1e-12 &&
     Tuple(one(RotMatrix{2,Float64})) == (1.0, 0.0, 0.0, 1.0) &&
     isrotation(r)

println(ok)
ok
