using StaticArrays

# Matrix * vector returns a static vector (Issue #7461).
A = @SMatrix [1.0 2.0; 3.0 4.0]
v = @SVector [5.0, 6.0]
r = A * v

# Affine map W*x + b, the IFS chaos-game kernel that motivated #7461 / #7949.
B = @SMatrix [0.85 0.04; -0.04 0.85]
x = @SVector [1.0, 2.0]
b = @SVector [0.0, 1.6]
y = B * x + b

# Scalar scaling (both orders) and elementwise subtraction.
z = 2.0 * v
w = v * 0.5
d = v - (@SVector [1.0, 1.0])

# 3x3 matrix times a 3-vector.
M = @SMatrix [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]
u = @SVector [1.0, 0.0, -1.0]
p = M * u

# 4x4 matrix times a 4-vector plus size-4 vector +/- and scalar scaling
# exercise the n == 4 hand-unrolled fast path (Issue #7956).
N4 = @SMatrix [1.0 2.0 3.0 4.0; 5.0 6.0 7.0 8.0; 9.0 10.0 11.0 12.0; 13.0 14.0 15.0 16.0]
g = @SVector [1.0, 0.0, 0.0, -1.0]
q = N4 * g
q4 = (@SVector [1.0, 2.0, 3.0, 4.0]) + (@SVector [10.0, 20.0, 30.0, 40.0])
qm = (@SVector [10.0, 20.0, 30.0, 40.0]) - (@SVector [1.0, 2.0, 3.0, 4.0])
qs = 3.0 * (@SVector [1.0, 2.0, 3.0, 4.0])

ok = typeof(r) == SVector{2, Float64} &&
     r[1] == 17.0 && r[2] == 39.0 &&
     length(r) == 2 && eltype(r) == Float64 &&
     y[1] == 0.9299999999999999 && y[2] == 3.26 &&
     z[1] == 10.0 && z[2] == 12.0 &&
     w[1] == 2.5 && w[2] == 3.0 &&
     d[1] == 4.0 && d[2] == 5.0 &&
     typeof(p) == SVector{3, Float64} &&
     p[1] == -2.0 && p[2] == -2.0 && p[3] == -2.0 &&
     typeof(q) == SVector{4, Float64} &&
     q[1] == -3.0 && q[2] == -3.0 && q[3] == -3.0 && q[4] == -3.0 &&
     q4[1] == 11.0 && q4[4] == 44.0 &&
     qm[1] == 9.0 && qm[4] == 36.0 &&
     qs[1] == 3.0 && qs[4] == 12.0

println((r[1], r[2], y[1], y[2], p[1], p[2], p[3], q[4], q4[4], qm[4], qs[4], ok))
ok
