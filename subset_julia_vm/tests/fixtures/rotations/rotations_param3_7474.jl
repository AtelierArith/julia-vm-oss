# Phase 3 (Issue #7474): the three-parameter 3-D rotation parametrisations
# RotationVec, RodriguesParam and MRP. Each realises its rotation matrix through
# a unit quaternion (matching upstream), so all three are checked against the
# equivalent RotZ rotation matrix.
using Rotations
using StaticArrays

approx(a, b) = all(abs.(a .- b) .< 1e-10)

v = SVector(1.0, 2.0, 3.0)
θ = 0.5
z = Tuple(RotZ(θ))          # reference matrix for a rotation about +z by θ

# RotationVec: axis*angle = (0, 0, θ)
rv = RotationVec(0.0, 0.0, θ)
# RodriguesParam: g = tan(θ/2) * axis
rp = RodriguesParam(0.0, 0.0, tan(θ / 2))
# MRP: p = tan(θ/4) * axis
mrp = MRP(0.0, 0.0, tan(θ / 4))

ok = rv isa RotationVec{Float64} &&
     rp isa RodriguesParam{Float64} &&
     mrp isa MRP{Float64} &&
     # all three describe the same rotation as RotZ(θ)
     approx(Tuple(rv), z) &&
     approx(Tuple(rp), z) &&
     approx(Tuple(mrp), z) &&
     # rotating a vector matches RotZ
     approx(Tuple(rv * v), Tuple(RotZ(θ) * v)) &&
     approx(Tuple(rp * v), Tuple(RotZ(θ) * v)) &&
     approx(Tuple(mrp * v), Tuple(RotZ(θ) * v)) &&
     # angle recovery
     abs(rotation_angle(rv) - θ) < 1e-10 &&
     abs(rotation_angle(rp) - θ) < 1e-10 &&
     abs(rotation_angle(mrp) - θ) < 1e-10 &&
     # axis recovery (RotationVec gives an exact unit axis)
     approx(Tuple(rotation_axis(rv)), (0.0, 0.0, 1.0)) &&
     # params round-trip the stored fields
     approx(Tuple(Rotations.params(rv)), (0.0, 0.0, θ)) &&
     approx(Tuple(Rotations.params(rp)), (0.0, 0.0, tan(θ / 2))) &&
     approx(Tuple(Rotations.params(mrp)), (0.0, 0.0, tan(θ / 4))) &&
     # inverse round-trip
     approx(Tuple(rv * (inv(rv) * v)), Tuple(v)) &&
     approx(Tuple(rp * (inv(rp) * v)), Tuple(v)) &&
     approx(Tuple(mrp * (inv(mrp) * v)), Tuple(v)) &&
     # identity / size
     approx(Tuple(one(rv)), Tuple(RotZ(0.0))) &&
     size(rp) == (3, 3)

println(ok)
ok
