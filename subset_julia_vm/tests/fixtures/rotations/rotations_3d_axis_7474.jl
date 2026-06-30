# Phase 3 (Issue #7474): single-axis 3-D rotations RotX / RotY / RotZ.
# Deterministic subset of extern/Rotations.jl/test (no random loops).
using Rotations
using StaticArrays

approx(a, b) = all(abs.(a .- b) .< 1e-12)

th = 0.5
s, c = sincos(th)
v = SVector(1.0, 2.0, 3.0)

rx = RotX(th); ry = RotY(th); rz = RotZ(th)

ok = rx isa RotX{Float64} && ry isa RotY{Float64} && rz isa RotZ{Float64} &&
     # column-major tuples match the upstream closed forms
     approx(Tuple(rx), (1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c)) &&
     approx(Tuple(ry), (c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c)) &&
     approx(Tuple(rz), (c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0)) &&
     # vector rotation closed forms
     approx(Tuple(rx * v), (v[1], v[2]*c - v[3]*s, v[3]*c + v[2]*s)) &&
     approx(Tuple(ry * v), (v[1]*c + v[3]*s, v[2], v[3]*c - v[1]*s)) &&
     approx(Tuple(rz * v), (v[1]*c - v[2]*s, v[2]*c + v[1]*s, v[3])) &&
     # angle / axis
     rotation_angle(rx) == th && rotation_angle(ry) == th && rotation_angle(rz) == th &&
     Tuple(rotation_axis(rx)) == (1.0, 0.0, 0.0) &&
     Tuple(rotation_axis(ry)) == (0.0, 1.0, 0.0) &&
     Tuple(rotation_axis(rz)) == (0.0, 0.0, 1.0) &&
     # inverse and same-axis composition (angles add)
     approx(Tuple(inv(rx) * rx), (1.0,0.0,0.0, 0.0,1.0,0.0, 0.0,0.0,1.0)) &&
     approx(Tuple(RotX(0.3) * RotX(0.2)), Tuple(RotX(0.5))) &&
     # typed-eltype construction preserves T
     RotZ{Float32}(th) isa RotZ{Float32} &&
     # size and getindex
     size(rx) == (3, 3) && rx[2, 2] == c && rz[2, 1] == s

println(ok)
ok
