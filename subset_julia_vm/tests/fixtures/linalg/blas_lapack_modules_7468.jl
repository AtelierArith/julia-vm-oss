using LinearAlgebra

function _close(a, b)
    return abs(a - b) < 1.0e-8
end

cx = ComplexF64[1.0 + 2.0im]
cy = ComplexF64[3.0 + 4.0im]
dot_ok = _close(BLAS.dot([1.0, 2.0], [3.0, 4.0]), 11.0) &&
    _close(BLAS.dot(2, [1.0, 2.0], 1, [3.0, 4.0], 1), 11.0) &&
    _close(BLAS.dotu(cx, cy), -5.0 + 10.0im) &&
    _close(BLAS.dotc(cx, cy), 11.0 - 2.0im)

y = [3.0, 4.0]
BLAS.axpy!(2.0, [1.0, 2.0], y)
axpy_ok = _close(y[1], 5.0) && _close(y[2], 8.0)

ys = [10.0, 20.0, 30.0]
BLAS.axpy!(2, -1.0, [2.0, 4.0], 1, ys, 2)
strided_axpy_ok = _close(ys[1], 8.0) &&
    _close(ys[2], 20.0) &&
    _close(ys[3], 26.0)

sx = [1.0, 2.0]
BLAS.scal!(3.0, sx)
scal_ok = _close(sx[1], 3.0) && _close(sx[2], 6.0)

A = [1.0 2.0; 3.0 4.0]
gv_y = [0.0, 0.0]
BLAS.gemv!('N', 1.0, A, [1.0, 1.0], 0.0, gv_y)
gemv_ok = _close(gv_y[1], 3.0) && _close(gv_y[2], 7.0)

C = zeros(2, 2)
BLAS.gemm!('N', 'N', 1.0, A, A, 0.0, C)
gemm_ok = _close(C[1, 1], 7.0) &&
    _close(C[1, 2], 10.0) &&
    _close(C[2, 1], 15.0) &&
    _close(C[2, 2], 22.0)

D = [2.0 0.0; 0.0 4.0]
b = [2.0, 8.0]
x, factored, pivots = LAPACK.gesv!(copy(D), copy(b))
gesv_ok = _close(x[1], 1.0) &&
    _close(x[2], 2.0) &&
    _close(factored[1, 1], 2.0) &&
    _close(factored[2, 2], 4.0) &&
    pivots[1] == 1 &&
    pivots[2] == 2

lu_data, lu_pivots, lu_info = LAPACK.getrf!(copy(D))
getrf_ok = lu_info == 0 &&
    lu_pivots[1] == 1 &&
    lu_pivots[2] == 2 &&
    _close(lu_data[1, 1], 2.0) &&
    _close(lu_data[2, 2], 4.0)

dot_ok &&
    axpy_ok &&
    strided_axpy_ok &&
    scal_ok &&
    gemv_ok &&
    gemm_ok &&
    gesv_ok &&
    getrf_ok
