# Phase 3 (Issue #7474): AngleAxis (axis-angle) rotation.
using Rotations
using StaticArrays

approx(a, b) = all(abs.(a .- b) .< 1e-12)

# axis is renormalised to unit length
aa = AngleAxis(0.5, 1.0, 1.0, 0.0)
v = SVector(1.0, 2.0, 3.0)
ax = Tuple(rotation_axis(aa))

# Rodrigues closed form (column-major) for the unit axis at angle 0.5
s, c = sincos(0.5)
ux, uy, uz = ax
c1 = 1 - c
expected_tuple = (1 - c1*uy^2 - c1*uz^2,  c1*ux*uy + s*uz,        c1*ux*uz - s*uy,
                  c1*ux*uy - s*uz,        1 - c1*ux^2 - c1*uz^2,  c1*uy*uz + s*ux,
                  c1*ux*uz + s*uy,        c1*uy*uz - s*ux,        1 - c1*ux^2 - c1*uy^2)

ok = aa isa AngleAxis{Float64} &&
     # axis renormalised to unit length
     approx(ax, (1/sqrt(2), 1/sqrt(2), 0.0)) &&
     rotation_angle(aa) == 0.5 &&
     # matrix entries match Rodrigues' formula
     approx(Tuple(aa), expected_tuple) &&
     # params = (θ, x, y, z); `params` is not exported by upstream Rotations
     approx(Tuple(Rotations.params(aa)), (0.5, 1/sqrt(2), 1/sqrt(2), 0.0)) &&
     # rotating a vector then by the inverse returns it
     approx(Tuple(aa * (inv(aa) * v)), Tuple(v)) &&
     # AngleAxis about Z equals RotZ
     approx(Tuple(AngleAxis(0.3, 0.0, 0.0, 1.0)), Tuple(RotZ(0.3))) &&
     # the rotation preserves length
     abs(sum(abs2, aa * v) - sum(abs2, v)) < 1e-10 &&
     size(aa) == (3, 3)

println(ok)
ok
