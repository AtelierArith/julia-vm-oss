# Phase 4 (Issue #7475): QuatRotation (unit-quaternion rotation) and slerp.
using Rotations
using StaticArrays
using Quaternions

approx(a, b) = all(abs.(a .- b) .< 1e-10)

v = SVector(1.0, 2.0, 3.0)
θ = 0.5

# Quaternion for a rotation about +z by θ: (cos θ/2, 0, 0, sin θ/2)
q = QuatRotation(cos(θ / 2), 0.0, 0.0, sin(θ / 2))
zt = Tuple(RotZ(θ))

# the constructor renormalises a non-unit quaternion
q2 = QuatRotation(2.0 * cos(θ / 2), 0.0, 0.0, 2.0 * sin(θ / 2))

ok = q isa QuatRotation{Float64} &&
     # the four components are accessible as fields (.w/.x/.y/.z)
     approx((q.w, q.x, q.y, q.z), (cos(θ / 2), 0.0, 0.0, sin(θ / 2))) &&
     # matrix matches RotZ(θ)
     approx(Tuple(q), zt) &&
     # vector rotation matches RotZ
     approx(Tuple(q * v), Tuple(RotZ(θ) * v)) &&
     # renormalisation: q2 describes the same rotation as q
     approx(Tuple(q2), zt) &&
     # params = (w, x, y, z)
     approx(Tuple(Rotations.params(q)), (cos(θ / 2), 0.0, 0.0, sin(θ / 2))) &&
     # angle / axis recovery
     abs(rotation_angle(q) - θ) < 1e-10 &&
     approx(Tuple(rotation_axis(q)), (0.0, 0.0, 1.0)) &&
     # inverse round-trip
     approx(Tuple(q * (inv(q) * v)), Tuple(v)) &&
     # composition: rotating by θ twice equals rotating by 2θ
     approx(Tuple((q * q) * v), Tuple(RotZ(2θ) * v)) &&
     # identity / size
     approx(Tuple(one(q)), Tuple(RotZ(0.0))) &&
     size(q) == (3, 3) &&
     # slerp halfway between identity and q is the half rotation
     approx(Tuple(slerp(QuatRotation(1.0, 0.0, 0.0, 0.0), q, 0.5) * v),
            Tuple(RotZ(θ / 2) * v))

println(ok)
ok
