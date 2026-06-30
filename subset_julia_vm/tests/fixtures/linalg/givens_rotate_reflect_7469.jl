using LinearAlgebra

function _close(a, b)
    return abs(a - b) < 1.0e-8
end

G, r = givens(3.0, 4.0, 1, 2)
scalar_ok = G isa LinearAlgebra.Givens &&
    _close(G.c, 0.6) &&
    _close(G.s, 0.8) &&
    _close(r, 5.0)

v = [3.0, 4.0]
gv = G * v
vector_apply_ok = length(gv) == 2 &&
    _close(gv[1], 5.0) &&
    _close(gv[2], 0.0)

A = [1.0 2.0; 3.0 4.0]
B = copy(A)
lmul!(G, B)
matrix_apply_ok = _close(B[1, 1], 3.0) &&
    _close(B[1, 2], 4.4) &&
    _close(B[2, 1], 1.0) &&
    _close(B[2, 2], 0.8)

G2, r2 = givens(A, 1, 2, 1)
matrix_givens_ok = G2 isa LinearAlgebra.Givens &&
    _close(G2.c, 1.0 / sqrt(10.0)) &&
    _close(G2.s, 3.0 / sqrt(10.0)) &&
    _close(r2, sqrt(10.0))

x = [1.0, 2.0]
y = [3.0, 4.0]
rotate!(x, y, 0.6, 0.8)
rotate_ok = _close(x[1], 3.0) &&
    _close(x[2], 4.4) &&
    _close(y[1], 1.0) &&
    _close(y[2], 0.8)

xr = [1.0, 2.0]
yr = [3.0, 4.0]
reflect!(xr, yr, 0.6, 0.8)
reflect_ok = _close(xr[1], 3.0) &&
    _close(xr[2], 4.4) &&
    _close(yr[1], -1.0) &&
    _close(yr[2], -0.8)

scalar_ok &&
    vector_apply_ok &&
    matrix_apply_ok &&
    matrix_givens_ok &&
    rotate_ok &&
    reflect_ok
